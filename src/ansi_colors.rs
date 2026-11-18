use ansi_colours::{ansi256_from_rgb, rgb_from_ansi256};
use std::{
    collections::HashMap,
    io::{self, Error, Write},
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

/// Convert colors to the nearest color of a color depth.
///
/// # Arguments
/// * `colors` - Colors to convert in place
/// * `depth` - Color depth to convert to
pub fn reduce_depth(colors: &mut HashMap<char, Color>, depth: ColorDepth) {
    for col in colors.values_mut() {
        *col = match depth {
            ColorDepth::True => *col,
            ColorDepth::Fixed256 => Fixed(ansi256(*col)),
            ColorDepth::Ansi16 => ansi16(*col),
        };
    }
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

pub fn ansi_byte(c: char) -> [u8; 1] {
    let mut b = [0; 1];
    c.encode_utf8(&mut b);
    b
}

pub fn write_ansi(
    buf: &mut (impl Write + ?Sized),
    styles: &HashMap<char, Style>,
    text: &str,
) -> io::Result<usize> {
    let reset = "\x1B[0m".as_bytes();
    let mut n_bytes = 0;
    // Only call reset when necessary (only when streaming).
    let mut fg = false;
    let mut bg = false;
    for c in text.chars() {
        match styles.get(&c) {
            Some(style) => {
                let _fg = style.foreground.is_some();
                let _bg = style.background.is_some();
                if (fg && !_fg) || (bg && !_bg) {
                    n_bytes += buf.write(reset)?;
                }
                fg = _fg;
                bg = _bg;
                n_bytes += buf.write(style.prefix().as_bytes())?;
                n_bytes += buf.write(&ansi_byte(c))?;
            }
            None => {
                n_bytes += buf.write(reset)?;
                n_bytes += buf.write(&ansi_byte(c))?;
            }
        };
    }
    n_bytes += buf.write(reset)?;
    Ok(n_bytes)
}

// To easily distinguish between formatted chars of sequences and any other text.
// Why not use Painted with no style? Because coloring might be disabled while we still may want to
// recognise a char as being part of a sequence.
pub enum Char {
    Styled(Painted<char>),
    Unstyled(char),
}

impl Char {
    pub fn write(&self, buf: &mut (impl Write + ?Sized)) -> Result<(), Error> {
        match &self {
            Char::Styled(painted) => buf.write_all(painted.to_string().as_bytes()),
            Char::Unstyled(c) => buf.write_all(&ansi_byte(*c)),
        }
    }

    fn style(&self) -> Style {
        match self {
            Char::Styled(painted) => painted.style,
            Char::Unstyled(_) => Style::new(),
        }
    }

    fn value(&self) -> char {
        match self {
            Char::Styled(painted) => painted.value,
            Char::Unstyled(c) => *c,
        }
    }
}

/// Write a line of Chars efficiently by:
/// 1. Batching consecutive same-styled characters
/// 2. Only emitting color changes (not full reset+set) when possible
///
/// This significantly reduces ANSI escape sequences for typical sequences.
/// Each line starts with a reset to ensure correct rendering when pagers
/// like `less` re-render individual lines during scrolling.
pub fn write_line(buf: &mut (impl Write + ?Sized), chars: &[Char]) -> Result<(), Error> {
    if chars.is_empty() {
        return Ok(());
    }

    // Reset at start of line ensures correct rendering when pagers re-render
    // individual lines in isolation (e.g., during mouse scroll in less).
    buf.write_all(b"\x1B[0m")?;

    // Track current terminal state to emit minimal escape sequences
    let mut current = Style::new();

    let mut i = 0;
    while i < chars.len() {
        let style = chars[i].style();

        // Collect consecutive chars with the same style
        let mut batch = String::new();
        while i < chars.len() && chars[i].style() == style {
            batch.push(chars[i].value());
            i += 1;
        }

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
            // Separate sequences for fg and bg, as a combined prefix() would write bg first.
            if style.foreground != current.foreground
                && let Some(fg) = style.foreground
            {
                buf.write_all(Style::new().fg(fg).prefix().as_bytes())?;
            }
            if style.background != current.background
                && let Some(bg) = style.background
            {
                buf.write_all(Style::new().bg(bg).prefix().as_bytes())?;
            }
        } else {
            if current != Style::new() {
                buf.write_all(b"\x1B[0m")?;
            }
            buf.write_all(style.prefix().as_bytes())?;
        }
        current = style;

        // Write the actual characters
        buf.write_all(batch.as_bytes())?;
    }

    // Reset at end of line if we have any active styling
    if current != Style::new() {
        buf.write_all(b"\x1B[0m")?;
    }

    Ok(())
}

pub fn to_painted(styles: &HashMap<char, Style>, text: &str) -> impl Iterator<Item = Char> {
    text.chars().map(|c| to_painted_char(styles, c))
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
            "\x1b[0m\x1b[31m\x1b[44mA\x1b[0m\x1b[31mA\x1b[0mA\x1b[44mA\x1b[0m",
        );
        // A primary color clears the color before it.
        assert_eq!(write_line_to_string(&[red, Style::new().fg(Primary)]), "\x1b[0m\x1b[31mA\x1b[39mA\x1b[0m");
    }
}
