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
pub fn write_line(buf: &mut (impl Write + ?Sized), chars: &[Char]) -> Result<(), Error> {
    if chars.is_empty() {
        return Ok(());
    }

    // Track current terminal state to emit minimal escape sequences
    let mut current_fg: Option<Color> = None;
    let mut current_bg: Option<Color> = None;

    let mut i = 0;
    while i < chars.len() {
        let style = chars[i].style();
        let new_fg = style.foreground;
        let new_bg = style.background;

        // Collect consecutive chars with the same style
        let mut batch = String::new();
        while i < chars.len() && chars[i].style() == style {
            batch.push(chars[i].value());
            i += 1;
        }

        // Emit minimal escape sequences based on what changed
        let need_reset = (current_fg.is_some() && new_fg.is_none())
            || (current_bg.is_some() && new_bg.is_none());

        if need_reset {
            buf.write_all(b"\x1B[0m")?;
            current_fg = None;
            current_bg = None;
        }

        // Only emit fg if it changed
        if new_fg != current_fg {
            if let Some(fg) = new_fg {
                write_fg(buf, fg)?;
            }
            current_fg = new_fg;
        }

        // Only emit bg if it changed
        if new_bg != current_bg {
            if let Some(bg) = new_bg {
                write_bg(buf, bg)?;
            }
            current_bg = new_bg;
        }

        // Write the actual characters
        buf.write_all(batch.as_bytes())?;
    }

    // Reset at end of line if we have any active styling
    if current_fg.is_some() || current_bg.is_some() {
        buf.write_all(b"\x1B[0m")?;
    }

    Ok(())
}

/// Write foreground color escape sequence
fn write_fg(buf: &mut (impl Write + ?Sized), color: Color) -> Result<(), Error> {
    match color {
        Rgb(r, g, b) => write!(buf, "\x1B[38;2;{r};{g};{b}m"),
        Fixed(n) => write!(buf, "\x1B[38;5;{n}m"),
        Black => buf.write_all(b"\x1B[30m"),
        Red => buf.write_all(b"\x1B[31m"),
        Green => buf.write_all(b"\x1B[32m"),
        Yellow => buf.write_all(b"\x1B[33m"),
        Blue => buf.write_all(b"\x1B[34m"),
        Magenta => buf.write_all(b"\x1B[35m"),
        Cyan => buf.write_all(b"\x1B[36m"),
        White => buf.write_all(b"\x1B[37m"),
        BrightBlack => buf.write_all(b"\x1B[90m"),
        BrightRed => buf.write_all(b"\x1B[91m"),
        BrightGreen => buf.write_all(b"\x1B[92m"),
        BrightYellow => buf.write_all(b"\x1B[93m"),
        BrightBlue => buf.write_all(b"\x1B[94m"),
        BrightMagenta => buf.write_all(b"\x1B[95m"),
        BrightCyan => buf.write_all(b"\x1B[96m"),
        BrightWhite => buf.write_all(b"\x1B[97m"),
        Primary => Ok(()), // Default, no code needed
    }
}

/// Write background color escape sequence
fn write_bg(buf: &mut (impl Write + ?Sized), color: Color) -> Result<(), Error> {
    match color {
        Rgb(r, g, b) => write!(buf, "\x1B[48;2;{r};{g};{b}m"),
        Fixed(n) => write!(buf, "\x1B[48;5;{n}m"),
        Black => buf.write_all(b"\x1B[40m"),
        Red => buf.write_all(b"\x1B[41m"),
        Green => buf.write_all(b"\x1B[42m"),
        Yellow => buf.write_all(b"\x1B[43m"),
        Blue => buf.write_all(b"\x1B[44m"),
        Magenta => buf.write_all(b"\x1B[45m"),
        Cyan => buf.write_all(b"\x1B[46m"),
        White => buf.write_all(b"\x1B[47m"),
        BrightBlack => buf.write_all(b"\x1B[100m"),
        BrightRed => buf.write_all(b"\x1B[101m"),
        BrightGreen => buf.write_all(b"\x1B[102m"),
        BrightYellow => buf.write_all(b"\x1B[103m"),
        BrightBlue => buf.write_all(b"\x1B[104m"),
        BrightMagenta => buf.write_all(b"\x1B[105m"),
        BrightCyan => buf.write_all(b"\x1B[106m"),
        BrightWhite => buf.write_all(b"\x1B[107m"),
        Primary => Ok(()), // Default, no code needed
    }
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
