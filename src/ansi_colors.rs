use std::collections::HashMap;
use ansi_colours::{ansi256_from_rgb, rgb_from_ansi256};
// For abstracting away writing ANSI codes.
use yansi::{
    Color::{self, *},
    Paint, Painted, Style,
};
use phf::phf_map;

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
        Rgb(r, g, b) => ansi256_from_rgb(&[r, g, b]),
        Primary => 15, // not known but not used
    }
}

pub fn print_ansi(styles: &HashMap<char, Style>, text: &str) {
    for c in text.chars() {
        print_ansi_char(styles, c);
    }
}
fn print_ansi_char(styles: &HashMap<char, Style>, c: char) {
    match styles.get(&c) {
        Some(style) => print!("{}", c.paint(*style)),
        None => print!("{}", c),
    }
}

pub fn to_painted(styles: &HashMap<char, Style>, text: &str) -> impl Iterator<Item = Painted<char>> {
    text.chars().map(|c| to_painted_char(styles, c))
}

fn to_painted_char(styles: &HashMap<char, Style>, c: char) -> Painted<char> {
    let style = match styles.get(&c) {
        Some(&style) => style,
        None => Style::new()
    };
    Painted { value: c, style }
}

pub fn is_styled(painted: &Painted<char>) -> bool {
    painted.style != Style::new()
}
