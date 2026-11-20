use ansi_colours::{ansi256_from_rgb, rgb_from_ansi256};
use regex::bytes::Regex;
use std::{
    collections::HashMap,
    io::{Error, Write},
    ops::Range,
    sync::LazyLock,
};
// For abstracting away writing ANSI codes.
use phf::phf_map;
use yansi::{
    Attribute,
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
    // Return whether a color is light or dark.
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
        // perceived lightness of a color.
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

    // Approximate RGB values of ANSI_COLORS
    const RGBS: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (128, 0, 0),
        (0, 128, 0),
        (128, 128, 0),
        (0, 0, 128),
        (128, 0, 128),
        (0, 128, 128),
        (192, 192, 192),
        (128, 128, 128),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (0, 0, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];

    // Find nearest color by Euclidean distance
    let mut best = Black;
    let mut best_dist = u32::MAX;
    for (color, (cr, cg, cb)) in ANSI_COLORS.into_iter().zip(RGBS) {
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

/// Styles a terminal can show.
#[derive(Debug, Clone, Copy)]
pub enum Support {
    /// No styles.
    Plain,
    /// Attributes, such as bold and underline, without colors.
    Attributes,
    /// Attributes and colors.
    Colors(ColorDepth),
}

/// Restrict a style to the styles a terminal can show.
///
/// # Arguments
/// * `style` - Style to restrict
/// * `support` - Styles the terminal can show
///
/// # Returns
/// No style for [`Support::Plain`], only the attributes of `style` for [`Support::Attributes`],
/// else `style` with its colors converted to the nearest colors of the color depth.
pub fn restrict(mut style: Style, support: Support) -> Style {
    let depth = match support {
        Support::Plain => return Style::new(),
        Support::Attributes => {
            style.foreground = None;
            style.background = None;
            return style;
        }
        Support::Colors(depth) => depth,
    };
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

/// The 16 basic ANSI colors, in the order of their SGR codes: 30 to 37, then 90 to 97 for foregrounds.
const ANSI_COLORS: [Color; 16] = [
    Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
    BrightBlack, BrightRed, BrightGreen, BrightYellow, BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
];

/// The attributes of SGR codes 1 to 9, in order.
const ATTRIBUTES: [Attribute; 9] = [
    Attribute::Bold,
    Attribute::Dim,
    Attribute::Italic,
    Attribute::Underline,
    Attribute::Blink,
    Attribute::RapidBlink,
    Attribute::Invert,
    Attribute::Conceal,
    Attribute::Strike,
];

/// CSI escape sequence (ECMA-48): `ESC [`, parameter bytes, intermediate bytes, final byte.
static CSI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?-u)\x1b\[([0-?]*)([ -/]*)([@-~])").unwrap());

/// Text without its CSI escape sequences, and the style of each part of it.
pub struct StyledText {
    /// Text without CSI sequences. Other bytes, also invalid UTF-8 and a lone ESC, are unchanged.
    pub text: Vec<u8>,
    /// Strictly ascending byte offsets of `text` where a style starts, each with the style. The first offset is 0.
    /// Codes at the same offset give one entry. The last offset can be `text.len()`.
    /// An offset can be inside a UTF-8 sequence, which then decodes as invalid parts.
    pub styles: Vec<(usize, Style)>,
}

impl StyledText {
    /// Remove the CSI escape sequences of `line`, and apply its SGR codes.
    /// An SGR code with `:` sub-parameters or private markers is removed without a style change.
    ///
    /// # Arguments
    /// * `line` - Bytes of one line
    /// * `style` - Style at the start of `line`
    ///
    /// # Returns
    /// The text and its styles. The style at the end of `line` is the last style.
    pub fn parse(line: Vec<u8>, mut style: Style) -> StyledText {
        let mut styles = vec![(0, style)];
        if !line.contains(&0x1b) {
            return StyledText { text: line, styles };
        }
        let mut text = Vec::with_capacity(line.len());
        let mut i = 0;
        for csi in CSI.captures_iter(&line) {
            let code = csi.get(0).unwrap();
            text.extend_from_slice(&line[i..code.start()]);
            i = code.end();
            let params = &csi[1];
            if &csi[3] != b"m" || !csi[2].is_empty() || !params.iter().all(|&b| b.is_ascii_digit() || b == b';') {
                continue;
            }
            style = apply_sgr(style, params);
            match styles.last_mut() {
                Some((offset, last)) if *offset == text.len() => *last = style,
                _ => styles.push((text.len(), style)),
            }
        }
        text.extend_from_slice(&line[i..]);
        StyledText { text, styles }
    }

    /// Chars of a part of the text, each with its style.
    ///
    /// # Arguments
    /// * `range` - Byte range of `self.text`
    ///
    /// # Returns
    /// [`Char::Text`] of each valid char of `range`, and [`Char::Invalid`] of each invalid sequence.
    pub fn chars(&self, range: Range<usize>) -> impl Iterator<Item = Char> + '_ {
        let i_first = self.styles.partition_point(|&(offset, _)| offset <= range.start) - 1;
        let parts = &self.styles[i_first..];
        let ends = parts[1..].iter().map(|&(offset, _)| offset).chain([self.text.len()]);
        parts
            .iter()
            .zip(ends)
            .take_while(move |&(&(start, _), _)| start < range.end)
            .flat_map(move |(&(start, style), end)| {
                decode(&self.text[start.max(range.start)..end.min(range.end)], move |c| Char::Text(Painted { value: c, style }))
            })
    }
}

/// Apply the parameters of an SGR code.
///
/// # Arguments
/// * `style` - Style before the code
/// * `params` - `;`-separated decimal parameters, only ASCII digits and `;`. An empty parameter is 0.
///
/// # Returns
/// `style` changed by each known parameter. A 38 or 48 with a bad or short color ends the code.
fn apply_sgr(mut style: Style, params: &[u8]) -> Style {
    let mut params = params
        .split(|&b| b == b';')
        .map(|digits| digits.iter().try_fold(0u32, |n, &d| n.checked_mul(10)?.checked_add(u32::from(d - b'0'))));
    while let Some(param) = params.next() {
        style = match param {
            Some(0) => Style::new(),
            Some(n @ 1..=9) => style.attr(ATTRIBUTES[n as usize - 1]),
            Some(22) => without_attrs(style, &[Attribute::Bold, Attribute::Dim]),
            Some(23) => without_attrs(style, &[Attribute::Italic]),
            Some(24) => without_attrs(style, &[Attribute::Underline]),
            Some(25) => without_attrs(style, &[Attribute::Blink, Attribute::RapidBlink]),
            Some(27) => without_attrs(style, &[Attribute::Invert]),
            Some(28) => without_attrs(style, &[Attribute::Conceal]),
            Some(29) => without_attrs(style, &[Attribute::Strike]),
            Some(n @ 30..=37) => style.fg(ANSI_COLORS[n as usize - 30]),
            Some(n @ 90..=97) => style.fg(ANSI_COLORS[n as usize - 90 + 8]),
            Some(n @ 40..=47) => style.bg(ANSI_COLORS[n as usize - 40]),
            Some(n @ 100..=107) => style.bg(ANSI_COLORS[n as usize - 100 + 8]),
            Some(38) => match read_color(&mut params) {
                Some(color) => style.fg(color),
                None => break,
            },
            Some(48) => match read_color(&mut params) {
                Some(color) => style.bg(color),
                None => break,
            },
            Some(39) => {
                style.foreground = None;
                style
            }
            Some(49) => {
                style.background = None;
                style
            }
            _ => style,
        };
    }
    style
}

/// Read the color of an SGR 38 or 48 parameter: `5;n` or `2;r;g;b`.
/// Consumes the parameters of the color from `params`.
///
/// # Arguments
/// * `params` - The parameters after 38 or 48. `None` is a parameter that overflows `u32`.
///
/// # Returns
/// The color, or `None` for a bad or short color.
fn read_color(params: &mut impl Iterator<Item = Option<u32>>) -> Option<Color> {
    let kind = params.next()??;
    let mut component = || u8::try_from(params.next()??).ok();
    match kind {
        5 => Some(Fixed(component()?)),
        2 => Some(Rgb(component()?, component()?, component()?)),
        _ => None,
    }
}

/// `style` without `attributes`. Colors are kept.
fn without_attrs(style: Style, attributes: &[Attribute]) -> Style {
    let mut colors = Style::new();
    colors.foreground = style.foreground;
    colors.background = style.background;
    ATTRIBUTES
        .into_iter()
        .filter(|a| !attributes.contains(a) && style.attr(*a) == style)
        .fold(colors, Style::attr)
}

/// A char of a line, or an invalid UTF-8 sequence.
/// A char is a [`Char::Match`] or a [`Char::Text`] by whether a match contains it, not by its style:
/// with styles disabled, the chars of matches are still known.
#[derive(Clone)]
pub enum Char {
    /// Char of a match.
    Match(Painted<char>),
    /// Char outside the matches, with its input style.
    Text(Painted<char>),
    /// An invalid UTF-8 sequence, unstyled. Written unchanged.
    Invalid(Box<[u8]>),
}

impl Char {
    /// Write the char without its style.
    ///
    /// # Arguments
    /// * `buf` - Destination
    ///
    /// # Errors
    /// The first write error.
    fn write_text(&self, buf: &mut (impl Write + ?Sized)) -> Result<(), Error> {
        match self {
            Char::Match(Painted { value: c, .. }) | Char::Text(Painted { value: c, .. }) => {
                buf.write_all(c.encode_utf8(&mut [0; 4]).as_bytes())
            }
            Char::Invalid(bytes) => buf.write_all(bytes),
        }
    }

    fn style(&self) -> Style {
        match self {
            Char::Match(painted) | Char::Text(painted) => painted.style,
            Char::Invalid(_) => Style::new(),
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
/// A styled line starts with a reset, and ends with a reset if it ends styled.
/// A line without styles is written unchanged.
///
/// # Arguments
/// * `buf` - Destination
/// * `chars` - Chars of the line
///
/// # Errors
/// The first write error.
pub fn write_line(buf: &mut (impl Write + ?Sized), chars: &[Char]) -> Result<(), Error> {
    // Reset at start of line ensures correct rendering when pagers re-render
    // individual lines in isolation (e.g., during mouse scroll in less).
    if chars.iter().any(|c| c.style() != Style::new()) {
        buf.write_all(b"\x1B[0m")?;
    }

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
    Char::Match(Painted { value: c, style })
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
    fn restrict_keeps_what_the_terminal_shows() {
        let style = Style::new().bold().fg(Rgb(255, 0, 0)).bg(Rgb(128, 128, 128));
        assert_eq!(restrict(style, Support::Plain), Style::new());
        assert_eq!(restrict(style, Support::Attributes), Style::new().bold());
        assert_eq!(
            restrict(style, Support::Colors(ColorDepth::Ansi16)),
            Style::new().bold().fg(ansi16(Rgb(255, 0, 0))).bg(ansi16(Rgb(128, 128, 128))),
        );
        assert_eq!(restrict(style, Support::Colors(ColorDepth::True)), style);
    }

    fn write_line_to_string(styles: &[Style]) -> String {
        let chars: Vec<Char> = styles.iter().map(|&style| Char::Match(Painted { value: 'A', style })).collect();
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
    fn write_line_writes_unstyled_chars_unchanged() {
        assert_eq!(write_line_to_string(&[Style::new(), Style::new()]), "AA");
        let mut output = Vec::new();
        write_line(&mut output, &[Char::Text(Painted { value: 'A', style: Style::new() }), Char::Invalid(Box::new([0xff]))]).unwrap();
        assert_eq!(output, b"A\xff");
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
        assert_eq!(decode(b"A\xe2\x82\xff", |c| Char::Text(Painted { value: c, style: Style::new() })).count(), 3);
    }

    fn parse(line: &[u8]) -> StyledText {
        StyledText::parse(line.to_vec(), Style::new())
    }

    #[test]
    fn parse_removes_codes_and_keeps_styles() {
        let styled = parse(b"a\x1b[1;31mb\x1b[0mc");
        assert_eq!(styled.text, b"abc");
        assert_eq!(styled.styles, [(0, Style::new()), (1, Style::new().bold().fg(Red)), (2, Style::new())]);
    }

    #[test]
    fn parse_reads_fixed_and_rgb_colors() {
        assert_eq!(parse(b"\x1b[38;5;196ma").styles, [(0, Style::new().fg(Fixed(196)))]);
        assert_eq!(parse(b"\x1b[48;2;1;2;3ma").styles, [(0, Style::new().bg(Rgb(1, 2, 3)))]);
    }

    #[test]
    fn parse_empty_params_reset() {
        assert_eq!(StyledText::parse(b"\x1b[ma".to_vec(), Style::new().bold()).styles, [(0, Style::new())]);
    }

    #[test]
    fn parse_removes_attribute() {
        let styled = parse(b"\x1b[1;4;31ma\x1b[22mb");
        assert_eq!(styled.styles, [(0, Style::new().bold().underline().fg(Red)), (1, Style::new().underline().fg(Red))]);
    }

    #[test]
    fn parse_removes_other_csi() {
        let styled = parse(b"a\x1b[Kb");
        assert_eq!(styled.text, b"ab");
        assert_eq!(styled.styles, [(0, Style::new())]);
    }

    #[test]
    fn parse_reads_grep_output() {
        let styled = parse(b"a\x1b[01;31m\x1b[Kb\x1b[m\x1b[K");
        assert_eq!(styled.text, b"ab");
        assert_eq!(styled.styles, [(0, Style::new()), (1, Style::new().bold().fg(Red)), (2, Style::new())]);
    }

    #[test]
    fn parse_keeps_invalid_utf8_and_lone_esc() {
        assert_eq!(parse(b"\xff\x1bx").text, b"\xff\x1bx");
    }

    #[test]
    fn parse_starts_with_given_style() {
        assert_eq!(StyledText::parse(b"a".to_vec(), Style::new().bold()).styles, [(0, Style::new().bold())]);
    }

    #[test]
    fn write_line_writes_non_ascii_and_invalid_utf8() {
        let styles = HashMap::from([('é', Style::new().fg(Red))]);
        let chars: Vec<Char> = to_painted(&styles, b"\xc3\xa9\xff\xc3\xb8").collect();
        let mut output = Vec::new();
        write_line(&mut output, &chars).unwrap();
        assert_eq!(output, b"\x1b[0m\x1b[31m\xc3\xa9\x1b[0m\xff\xc3\xb8");
    }
}
