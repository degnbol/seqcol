#![allow(unused_variables)]

use anyhow::Result;
use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::io::BufRead;
use std::process::exit;

// For detecting if terminal can show true colors, etc.
use anstyle_query;
// For abstracting away writing ANSI codes.
use yansi::{
    Color::{self, *},
    Paint, Painted, Style,
};

use ansi_colours::{ansi256_from_rgb, rgb_from_ansi256};

mod colorschemes;
mod inout;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    // Input file(s)
    #[arg(
        value_name = "FILE",
        default_value = "-",
        help = "Text containing sequences. Default is reading stdin."
    )]
    files: Vec<String>,

    #[arg(
        short('F'),
        long("no-fasta"),
        help = "By default lines starting with '>' are ignored."
    )]
    no_fasta_check: bool,

    #[arg(short('T'), long, help = "Transpose the input.")]
    transpose: bool,

    #[arg(short('m'), long("min"), help = "Minimum sequence length to color.")]
    min_seq_length: Option<u32>,

    #[arg(
        short('r'),
        long,
        conflicts_with("min_seq_length"), // Combining them can be implemented in future.
        help = "Color sequences matching the given regex."
    )]
    regex: Option<String>,

    #[arg(
        short('s'),
        long("scheme"),
        help = "Name of predefined colorscheme or file with custom colorscheme. \
        Flag can be specified multiple times where \
        definitions in subsequent color schemes take precedence over previous. \
        Use -l/--list-schemes to get list of available colorschemes. \
        Default is \"shapely_aa\". \
        In a colorscheme file, each line should contain a character, then a delimiter, e.g. tab or comma, then a color name, hex, or \
        integer triplet, e.g. delimiting integers with spaces or commas."
    )]
    colorscheme: Option<Vec<String>>,

    #[arg(
        short('S'),
        long("fg"),
        help = "Modify foreground colors given file(s) with same format as described for -s/--scheme. \
        By default text fg colors are white or black depending on lightness of background color, while gaps are gray. \
        Foreground color is also modified by -i/--invisible."
    )]
    foreground: Option<Vec<String>>,

    // TODO: see if there is a performance benefit to using primary term colours. If not, remove
    // this temp flag. If so, look back into best option for detection, and otherwise have manual
    // flag to set dark vs light terminal.
    // #[arg(
    //     short('b'),
    //     long("bw"),
    //     help = "Print sequence letters with black and white foreground, rather than using the terminals primary colors."
    // )]
    // blackwhite: bool,
    // TODO: consider usefulness and how this plays together with other options, e.g. consensus,
    // regex etc. What are the use cases?
    #[arg(
        short('i'),
        long,
        // using dot to mean any letter of the chosen alphabet. "" will mean nothing is invisible.
        default_missing_value("."),
        help = "Hide letter codes by printing text foreground color the same as background color. \
        Optionally follow flag by a string of letters to only make some letter invisible. \
        If the first char is \"^\" followed by other characters, then it's a reverse pattern."
    )]
    invisible: Option<String>,

    #[arg(
        short('c'),
        long("consensus"),
        value_parser(["bold", "underline"]),
        value_name("STYLE"),
        num_args(0..=1),
        require_equals(true),
        default_missing_value("bold"),
        help = "Highlight whether letters are of the consensus sequence. \
        Default: bold."
    )]
    // Only count letters from the alphabet at each position.
    // TODO: Option to only count within the matched min-length and regex,
    // or have separate regex option if that would ever be useful.
    // TODO: Currently excluding gap. Make an exclusion option.
    // Currently just makes bold.
    consensus: Option<String>,

    // TODO: implement this option.
    #[arg(
        short('C'),
        long("mut"),
        value_parser(["bold", "underline"]),
        value_name("STYLE"),
        num_args(0..=1),
        require_equals(true),
        default_missing_value("bold"),
        help = "Opposite of -c/--consensus. \
        Highlight mutations/deviations from consensus. \
        Default: bold, unless both -c/--consensus and -C/--mut are specified, \
        then consensus is underlined and mutations are bold."
    )]
    not_consensus: Option<String>,

    #[arg(
        short('l'),
        long("list-schemes"),
        help = "List available colorschemes."
    )]
    list_colorschemes: bool,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    if args.list_colorschemes {
        let names = colorschemes::get_colorscheme_names();
        println!("{}", names.join("\n"));
        exit(0)
    }

    let schemes = colorschemes::load_colorschemes();

    let colors: HashMap<char, Color> = match args.colorscheme {
        None => schemes.get("shapely_aa").unwrap().clone(),
        Some(scheme_names) => {
            let mut colors: HashMap<char, Color> = HashMap::new();
            for scheme_name in scheme_names {
                match schemes.get(&scheme_name) {
                    Some(_colors) => colors.extend(_colors),
                    None => colors.extend(
                        colorschemes::read_colorscheme(&scheme_name)
                            .expect("Colorscheme not understood"),
                    ),
                };
            }
            colors
        }
    };

    let mut ansi_colors = HashMap::new();
    if anstyle_query::truecolor() {
        for (c, col) in colors.into_iter() {
            ansi_colors.insert(c, col);
        }
    } else if anstyle_query::term_supports_ansi_color() {
        for (c, col) in colors.into_iter() {
            ansi_colors.insert(c, Fixed(ansi256(col)));
        }
    } else if anstyle_query::term_supports_color() {
        unimplemented!()
    } else {
        unimplemented!()
    }

    let mut styles = HashMap::new();

    // TEMP: dimmed gap builtin.
    styles.insert('-', Rgb(128, 128, 128).foreground());

    // Make text legible by using dark text with light bg, and light text with dark bg.
    // We can either explicitly set the text fg to black and white, or use inversion to use the
    // terminal colours. Here we wanted to do the latter but it breaks the pager.
    for (c, col) in ansi_colors.iter() {
        if is_light(*col) {
            styles.insert(*c, col.background().fg(Black));
        } else {
            styles.insert(*c, col.background().fg(White));
        }
    }

    match args.invisible {
        Some(invisible) => {
            if invisible == "." {
                for (c, col) in ansi_colors.into_iter() {
                    styles.insert(c, col.background().fg(col));
                }
            } else if invisible.starts_with("^") {
                let visible = &invisible[1..];
                for (c, col) in ansi_colors.into_iter() {
                    if !visible.contains(c) {
                        styles.insert(c, col.background().fg(col));
                    }
                }
            } else {
                for c in invisible.chars() {
                    let style = match ansi_colors.get(&c) {
                        Some(col) => col.background().fg(*col),
                        None => panic!("Invisible only supported for char with a chosen color."),
                    };
                    styles.insert(c, style);
                }
            }
        }
        None => {}
    }

    let re = match args.regex {
        None => None,
        Some(regex) => Some(Regex::new(regex.as_str()).expect("Uknown regex.")),
    };

    let re = match args.min_seq_length {
        None => re,
        Some(min_seq_length) => {
            // Build regex of min length of matches taken from the colorscheme alphabet.
            let mut alphabet = Vec::new();
            for c in styles.keys() {
                // characters with special meaning inside regex [...]
                if "^[]-".contains(*c) {
                    alphabet.push('\\');
                }
                alphabet.push(*c);
            }
            let alphabet: String = alphabet.iter().collect();
            Some(Regex::new(format!("[{alphabet}]{{{min_seq_length},}}").as_str()).unwrap())
        }
    };

    let calc_consensus = args.consensus.is_some();
    let streaming = !args.transpose && !calc_consensus;

    if streaming {
        for filename in args.files {
            match inout::open(&filename) {
                Err(e) => eprintln!("{filename}: {e}"),
                Ok(file) => {
                    for line_result in file.lines() {
                        let line = line_result?;
                        // In case of fasta, skip coloring lines starting with '>'
                        if !args.no_fasta_check && line.starts_with('>') {
                            println!("{}", line);
                        } else {
                            match &re {
                                None => {
                                    ansiprint(&styles, &line);
                                    println!();
                                }
                                Some(_re) => {
                                    let mut i = 0;
                                    for m in _re.find_iter(&line) {
                                        print!("{}", &line[i..m.start()]);
                                        ansiprint(&styles, m.as_str());
                                        i = m.end();
                                    }
                                    println!("{}", &line[i..]);
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Not streaming.
        // First read input into memory.
        let (lines, max_line) = inout::read_lines(args.files)?;

        // Gather styles according to each char in each line.
        let mut painted_lines: Vec<Vec<Painted<char>>> = Vec::with_capacity(lines.len());
        for line in &lines {
            if !args.no_fasta_check && line.starts_with('>') {
                painted_lines.push(line.chars().map(|c| Paint::new(c)).collect());
            } else {
                let mut painted_line: Vec<Painted<char>> = Vec::with_capacity(line.len());
                match &re {
                    None => {
                        for (i, c) in line.chars().enumerate() {
                            painted_line.push(get_painted(&styles, c));
                        }
                    }
                    Some(_re) => {
                        let mut i_char = 0;
                        for m in _re.find_iter(&line) {
                            let start = m.start();
                            for c in line[i_char..start].chars() {
                                painted_line.push(Paint::new(c));
                            }
                            for c in m.as_str().chars() {
                                painted_line.push(get_painted(&styles, c));
                            }
                            i_char = m.end();
                        }
                    }
                }
                painted_lines.push(painted_line);
            }
        }

        if calc_consensus {
            // Count char occurrences.
            let mut letter_counts: Vec<HashMap<char, i32>> = Vec::with_capacity(max_line);
            for _ in 0..max_line {
                letter_counts.push(HashMap::new());
            }
            for painted_line in &painted_lines {
                for (i, painted) in painted_line.iter().enumerate() {
                    let c = painted.value;
                    // Exclude gap. Currently hard-coded. TODO: allow for option to choose
                    // exclusion chars.
                    if c != '-' {
                        let _letter_counts = &mut letter_counts[i];
                        match _letter_counts.get(&c) {
                            None => _letter_counts.insert(c, 1),
                            Some(n) => _letter_counts.insert(c, n + 1),
                        };
                    }
                }
            }
            // Define consensus as string of chars seen with max occurrences at each location.
            let mut consensus: Vec<Option<char>> = Vec::with_capacity(max_line);
            for i in 0..max_line {
                let mut _consensus: Option<char> = None;
                let mut max = 0;
                for (c, n) in letter_counts[i].iter() {
                    if *n > max {
                        max = *n;
                        _consensus = Some(*c);
                    }
                }
                consensus.push(_consensus);
            }

            // Apply the effect to letters matching the consensus.
            for painted_line in &mut painted_lines {
                for (i, painted) in painted_line.iter_mut().enumerate() {
                    match consensus[i] {
                        None => {}
                        Some(_consensus) => {
                            if _consensus == painted.value {
                                painted.style = painted.style.bold();
                            }
                        }
                    }
                }
            }
        }

        if !args.transpose {
            for painted_line in &painted_lines {
                for painted in painted_line {
                    print!("{}", painted);
                }
                println!();
            }
        } else {
            // Transpose.
            for j in 0..max_line {
                for painted_line in &painted_lines {
                    match painted_line.get(j) {
                        None => print!(" "),
                        Some(painted) => print!("{}", painted),
                    }
                }
                println!();
            }
        }
    }
    Ok(())
}

fn is_light(col: Color) -> bool {
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

fn ansi256(col: Color) -> u8 {
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

fn ansiprint(styles: &HashMap<char, Style>, text: &str) {
    for c in text.chars() {
        _ansiprint(styles, c);
    }
}
fn _ansiprint(styles: &HashMap<char, Style>, c: char) {
    match styles.get(&c) {
        Some(style) => print!("{}", c.paint(*style)),
        None => print!("{}", c),
    }
}

fn get_painted(styles: &HashMap<char, Style>, c: char) -> Painted<char> {
    let mut painted = Paint::new(c);
    match styles.get(&c) {
        None => {}
        Some(style) => painted.style = *style,
    }
    painted
}
