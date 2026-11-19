use ansi_colours::{ansi256_from_rgb, rgb_from_ansi256};
use std::{
    collections::HashMap,
    io::{Error, Write},
};
// For abstracting away writing ANSI codes.
use phf::phf_map;
use yansi::{
    Color::{self, *},
    Painted, Style,
};

pub static COLOR_NAMES: phf::Map<&'static str, Color> = phf_map! {
    "black"         => Black,
    "red"           => Red,
    "green"         => Green,
    "yellow"        => Yellow,
    "blue"          => Blue,
    "magenta"       => Magenta,
    "cyan"          => Cyan,
    "white"         => White,
    "brightblack"   => BrightBlack,
    "brightred"     => BrightRed,
    "brightgreen"   => BrightGreen,
    "brightyellow"  => BrightYellow,
    "brightblue"    => BrightBlue,
    "brightmagenta" => BrightMagenta,
    "brightcyan"    => BrightCyan,
    "brightwhite"   => BrightWhite,
    "primary"       => Primary,
    "gray"          => Rgb(127, 127, 127),
    "grey"          => Rgb(127, 127, 127),
};

// Parse 6 char long hex string.
pub fn parse_hex(hex: &str) -> Color {
    let r = u8::from_str_radix(&hex[0..2], 16).expect(hex);
    let g = u8::from_str_radix(&hex[2..4], 16).expect(hex);
    let b = u8::from_str_radix(&hex[4..6], 16).expect(hex);
    Rgb(r, g, b)
}

pub fn is_light(col: Color) -> bool {
    // Return whether a colour is light or dark.
    match col {
        Black => false,
        Red => false,
        // Varies by program:
        // https://stackoverflow.com/questions/4842424/list-of-ansi-color-escape-sequences
        // We go with the most common, otherwise would have to test which program someone uses.
        Green => false,
        Yellow => false,
        Blue => false,
        Magenta => false,
        Cyan => false,
        White => true,
        BrightBlack => false,
        BrightRed => true,
        BrightGreen => true,
        BrightYellow => true,
        BrightBlue => true,
        BrightMagenta => true,
        BrightCyan => true,
        BrightWhite => true,
        Fixed(idx) => {
            let (r, g, b) = rgb_from_ansi256(idx);
            is_light(Rgb(r, g, b))
        }
        // Simple relative luminance calculation for roughly and efficiently approximating the
        // perceived lightness of a colour.
        Rgb(r, g, b) => r as f32 * 0.2126 + g as f32 * 0.7152 + b as f32 * 0.0722 > 128.,
        // Not sure how useful/meaningful, but here for completeness.
        Primary => false,
    }
}

/// Map any color to the nearest basic 16 ANSI color.
/// Returns one of: Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
/// or their Bright variants.
pub fn ansi16(col: Color) -> Color {
    let (r, g, b) = match col {
        // Already a basic color
        Black | Red | Green | Yellow | Blue | Magenta | Cyan | White |
        BrightBlack | BrightRed | BrightGreen | BrightYellow |
        BrightBlue | BrightMagenta | BrightCyan | BrightWhite | Primary => return col,
        Fixed(idx) => rgb_from_ansi256(idx),
        Rgb(r, g, b) => (r, g, b),
    };

    // Basic 16 color palette (approximate RGB values)
    const COLORS: [(Color, u8, u8, u8); 16] = [
        (Black, 0, 0, 0),
        (Red, 128, 0, 0),
        (Green, 0, 128, 0),
        (Yellow, 128, 128, 0),
        (Blue, 0, 0, 128),
        (Magenta, 128, 0, 128),
        (Cyan, 0, 128, 128),
        (White, 192, 192, 192),
        (BrightBlack, 128, 128, 128),
        (BrightRed, 255, 0, 0),
        (BrightGreen, 0, 255, 0),
        (BrightYellow, 255, 255, 0),
        (BrightBlue, 0, 0, 255),
        (BrightMagenta, 255, 0, 255),
        (BrightCyan, 0, 255, 255),
        (BrightWhite, 255, 255, 255),
    ];

    // Find nearest color by Euclidean distance
    let mut best = Black;
    let mut best_dist = u32::MAX;
    for (color, cr, cg, cb) in COLORS {
        let dr = (r as i32 - cr as i32).unsigned_abs();
        let dg = (g as i32 - cg as i32).unsigned_abs();
        let db = (b as i32 - cb as i32).unsigned_abs();
        let dist = dr * dr + dg * dg + db * db;
        if dist < best_dist {
            best_dist = dist;
            best = color;
        }
    }
    best
}

// Get the color code in range 0 to 255 for a given color.
pub fn ansi256(col: Color) -> u8 {
    match col {
        Black => 0,
        Red => 124,
        Green => 2,
        Yellow => 184,
        Blue => 12,
        Magenta => 90,
        Cyan => 43,
        White => 255,
        BrightBlack => 238,
        BrightRed => 9,
        BrightGreen => 40,
        BrightYellow => 11,
        BrightBlue => 33,
        BrightMagenta => 13,
        BrightCyan => 14,
        BrightWhite => 15,
        Fixed(idx) => idx,
        Rgb(r, g, b) => ansi256_from_rgb([r, g, b]),
        Primary => 15, // not known but not used
    }
}

/// Number of colors a terminal can show.
#[derive(Debug, Clone, Copy)]
pub enum ColorDepth {
    /// 24-bit RGB.
    True,
    /// 256-color palette.
    Fixed256,
    /// Basic 16 ANSI colors.
    Ansi16,
}

/// Convert the colors of a style to the nearest colors of a color depth.
///
/// # Arguments
/// * `style` - Style to convert. Attributes are kept.
/// * `depth` - Color depth to convert to
///
/// # Returns
/// The style with converted foreground and background colors.
pub fn reduce_depth(mut style: Style, depth: ColorDepth) -> Style {
    let reduce = |col: Color| match depth {
        ColorDepth::True => col,
        ColorDepth::Fixed256 => Fixed(ansi256(col)),
        ColorDepth::Ansi16 => ansi16(col),
    };
    style.foreground = style.foreground.map(reduce);
    style.background = style.background.map(reduce);
    style
}

/// Combine foreground and background colors into styles.
///
/// # Arguments
/// * `fg` - Foreground color of each character
/// * `bg` - Background color of each character
///
/// # Returns
/// Style of each character in `fg` or `bg`, with the colors it has in each.
pub fn combine(fg: &HashMap<char, Color>, bg: &HashMap<char, Color>) -> HashMap<char, Style> {
    let mut styles: HashMap<char, Style> = bg.iter().map(|(&c, &col)| (c, col.background())).collect();
    for (&c, &col) in fg {
        let style = styles.entry(c).or_default();
        *style = style.fg(col);
    }
    styles
}

/// Lay `top` over `base`.
///
/// # Arguments
/// * `base` - Style under `top`. Only its colors are kept.
/// * `top` - Style over `base`
///
/// # Returns
/// `top`, with the foreground and background of `base` where `top` has none.
pub fn overlay(base: Style, top: Style) -> Style {
    let mut style = top;
    style.foreground = top.foreground.or(base.foreground);
    style.background = top.background.or(base.background);
    style
}

// To easily distinguish between formatted chars of sequences and any other text.
// Why not use Painted with no style? Because coloring might be disabled while we still may want to
// recognise a char as being part of a sequence.
pub enum Char {
    Styled(Painted<char>),
    Unstyled(char),
    /// An invalid UTF-8 sequence, unstyled. Written unchanged.
    Invalid(Box<[u8]>),
}

impl Char {
    /// Write the char with its full style, then a reset if styled. See `Char::write_text` for the char only.
    ///
    /// # Errors
    /// The first write error.
    pub fn write(&self, buf: &mut (impl Write + ?Sized)) -> Result<(), Error> {
        match self {
            Char::Styled(painted) => buf.write_all(painted.to_string().as_bytes()),
            _ => self.write_text(buf),
        }
    }

    /// Write the char without its style.
    ///
    /// # Arguments
    /// * `buf` - Destination
    ///
    /// # Errors
    /// The first write error.
    fn write_text(&self, buf: &mut (impl Write + ?Sized)) -> Result<(), Error> {
        match self {
            Char::Styled(Painted { value: c, .. }) | Char::Unstyled(c) => buf.write_all(c.encode_utf8(&mut [0; 4]).as_bytes()),
            Char::Invalid(bytes) => buf.write_all(bytes),
        }
    }

    fn style(&self) -> Style {
        match self {
            Char::Styled(painted) => painted.style,
            _ => Style::new(),
        }
    }
}

/// Decode UTF-8 into chars.
///
/// # Arguments
/// * `text` - Bytes to decode
/// * `to_char` - Makes the [`Char`] of a valid char
///
/// # Returns
/// The chars of `text`. Each invalid sequence becomes one [`Char::Invalid`].
/// An invalid sequence is the unit that [`String::from_utf8_lossy`] replaces with one U+FFFD.
pub fn decode<'a>(text: &'a [u8], to_char: impl Fn(char) -> Char + Copy + 'a) -> impl Iterator<Item = Char> + 'a {
    text.utf8_chunks().flat_map(move |chunk| {
        let invalid = chunk.invalid();
        chunk.valid().chars().map(to_char).chain((!invalid.is_empty()).then(|| Char::Invalid(invalid.into())))
    })
}

/// Write chars with few escape codes: one style prefix for each run of same-styled chars,
/// with only the changed colors when possible.
///
/// # Arguments
/// * `buf` - Destination
/// * `chars` - Chars of the line. Empty writes nothing. Otherwise the line starts with a reset,
///   and ends with a reset if it ends styled.
///
/// # Errors
/// The first write error.
pub fn write_line(buf: &mut (impl Write + ?Sized), chars: &[Char]) -> Result<(), Error> {
    if chars.is_empty() {
        return Ok(());
    }

    // Reset at start of line ensures correct rendering when pagers re-render
    // individual lines in isolation (e.g., during mouse scroll in less).
    buf.write_all(b"\x1B[0m")?;

    // Track current terminal state to emit minimal escape sequences
    let mut current = Style::new();
    // Escape codes before the current style run.
    let mut prefix = String::new();

    let mut i = 0;
    while i < chars.len() {
        let style = chars[i].style();
        prefix.clear();

        // Emit only the changed colors if the attributes stay the same and no color is removed.
        // Else reset and emit the full style.
        let keeps_attributes = {
            let mut recolored = current;
            recolored.foreground = style.foreground;
            recolored.background = style.background;
            recolored == style
        };
        let drops_color = (current.foreground.is_some() && style.foreground.is_none())
            || (current.background.is_some() && style.background.is_none());

        if keeps_attributes && !drops_color {
            let mut changed = Style::new();
            if style.foreground != current.foreground {
                changed.foreground = style.foreground;
            }
            if style.background != current.background {
                changed.background = style.background;
            }
            let _ = changed.fmt_prefix(&mut prefix);
        } else {
            if current != Style::new() {
                prefix.push_str("\x1B[0m");
            }
            let _ = style.fmt_prefix(&mut prefix);
        }
        current = style;
        buf.write_all(prefix.as_bytes())?;

        // Write consecutive chars with the same style
        while i < chars.len() && chars[i].style() == style {
            chars[i].write_text(buf)?;
            i += 1;
        }
    }

    // Reset at end of line if we have any active styling
    if current != Style::new() {
        buf.write_all(b"\x1B[0m")?;
    }

    Ok(())
}

pub fn to_painted<'a>(styles: &'a HashMap<char, Style>, text: &'a [u8]) -> impl Iterator<Item = Char> + 'a {
    decode(text, move |c| to_painted_char(styles, c))
}

fn to_painted_char(styles: &HashMap<char, Style>, c: char) -> Char {
    let style = match styles.get(&c) {
        Some(&style) => style,
        None => Style::new(),
    };
    Char::Styled(Painted { value: c, style })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_fills_colors_from_base() {
        let base = Style::new().fg(Red).bg(Blue);
        assert_eq!(overlay(base, Style::new().bold()), Style::new().bold().fg(Red).bg(Blue));
        assert_eq!(overlay(base, Style::new().bg(Green)), Style::new().fg(Red).bg(Green));
    }

    #[test]
    fn reduce_depth_converts_colors_and_keeps_attributes() {
        let style = Style::new().bold().fg(Rgb(255, 0, 0)).bg(Rgb(128, 128, 128));
        assert_eq!(reduce_depth(style, ColorDepth::Ansi16), Style::new().bold().fg(ansi16(Rgb(255, 0, 0))).bg(ansi16(Rgb(128, 128, 128))));
        assert_eq!(reduce_depth(style, ColorDepth::True), style);
    }

    fn write_line_to_string(styles: &[Style]) -> String {
        let chars: Vec<Char> = styles.iter().map(|&style| Char::Styled(Painted { value: 'A', style })).collect();
        let mut output = Vec::new();
        write_line(&mut output, &chars).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn write_line_writes_attributes() {
        let bold = Style::new().bold();
        assert_eq!(write_line_to_string(&[bold]), "\x1b[0m\x1b[1mA\x1b[0m");
        assert_eq!(write_line_to_string(&[bold, Style::new()]), "\x1b[0m\x1b[1mA\x1b[0mA");
        assert_eq!(
            write_line_to_string(&[bold.fg(Red), bold.fg(Blue)]),
            "\x1b[0m\x1b[1;31mA\x1b[34mA\x1b[0m",
        );
        assert_eq!(write_line_to_string(&[bold.bg(Red), bold]), "\x1b[0m\x1b[1;41mA\x1b[0m\x1b[1mA\x1b[0m");
        assert_eq!(
            write_line_to_string(&[bold, Style::new().underline()]),
            "\x1b[0m\x1b[1mA\x1b[0m\x1b[4mA\x1b[0m",
        );
    }

    #[test]
    fn write_line_writes_colors() {
        let red = Style::new().fg(Red);
        assert_eq!(
            write_line_to_string(&[red.bg(Blue), red, Style::new(), Style::new().bg(Blue)]),
            "\x1b[0m\x1b[44;31mA\x1b[0m\x1b[31mA\x1b[0mA\x1b[44mA\x1b[0m",
        );
        // A primary color clears the color before it.
        assert_eq!(write_line_to_string(&[red, Style::new().fg(Primary)]), "\x1b[0m\x1b[31mA\x1b[39mA\x1b[0m");
    }

    #[test]
    fn decode_makes_one_char_for_each_invalid_sequence() {
        assert_eq!(decode(b"A\xe2\x82\xff", Char::Unstyled).count(), 3);
    }

    #[test]
    fn write_line_writes_non_ascii_and_invalid_utf8() {
        let styles = HashMap::from([('é', Style::new().fg(Red))]);
        let chars: Vec<Char> = to_painted(&styles, b"\xc3\xa9\xff\xc3\xb8").collect();
        let mut output = Vec::new();
        write_line(&mut output, &chars).unwrap();
        assert_eq!(output, b"\x1b[0m\x1b[31m\xc3\xa9\x1b[0m\xff\xc3\xb8");
    }

    #[test]
    fn char_write_writes_non_ascii() {
        let mut output = Vec::new();
        Char::Unstyled('ø').write(&mut output).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "ø");
    }
}
