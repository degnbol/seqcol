#![allow(unused_variables)]

use anyhow::Result;
use clap::Parser;
use regex::Regex;
use std::process::exit;
use std::{collections::HashMap, vec};

// For detecting if terminal can show true colors, etc.
use anstyle_query;
// For abstracting away writing ANSI codes.
use yansi::{
    Color::{self, *},
    Paint, Painted, Style,
};

mod ansi_colors;
mod colorschemes;
mod inout;

use crate::inout::read_lines;
use crate::{
    ansi_colors::{ansi256, is_light, print_ansi, to_painted,is_styled},
    colorschemes::parse_color,
};

#[derive(Debug, Parser)]
#[command(
    author = "Christian Madsen",
    version = "0.1.0",
    about = "Colourise sequences of characters based on the characters.",
    long_about = "Colourise biological sequences (amino acids, DNA, and RNA). \
    Useful for viewing fasta files, sequence alignments, CSV, TSV, and other text files. \
    A simple commandline tool like `cat`, which may be useful for colourising \
    sequence of characters in general."
)]
struct Args {
    // Input file(s)
    #[arg(
        value_name = "FILE",
        default_value = "-",
        help = "Text containing sequences. Default is reading stdin."
    )]
    files: Vec<String>,

    // Options controlling how to color.
    #[arg(
        short('s'),
        long("scheme"),
        help = "Name of predefined colorscheme or file with custom colorscheme to control background color for each given character. \
        Flag can be specified multiple times where \
        definitions in subsequent color schemes take precedence over previous. \
        Use -l/--list-schemes to get list of available colorschemes. \
        Default is \"shapely_aa\". \
        Colorscheme file format: each line contains a character and a color separated by a delimiter. The delimiter can be tab, comma, semicolon, etc.
        The color can be a color name, hex, or integer triplet delimited by spaces or commas."
    )]
    colorscheme: Option<Vec<String>>,

    #[arg(
        short('S'),
        long("fg"),
        help = "The same as -s/--scheme, except controls character foreground instead of background colors (the character itself). \
        By default each character is either white or black depending on lightness of their background color, while gaps are gray. \
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
        help = "Hide letter codes by showing text foreground color the same as background color. \
        If this is given as a flag, all characters recognised from the colorscheme are invisible. \
        An argument can be provided to indicate which characters to make invisible. \
        If prefixed by \"^\", then it's reversed. \
        Takes precedence over -S/--fg."
    )]
    invisible: Option<String>,

    // Options controlling what to color.
    #[arg(short('m'), long("min"), help = "Minimum sequence length to color.")]
    min_seq_length: Option<u32>,

    #[arg(
        short('r'),
        long,
        default_value = "^[^>@+].*",
        help = "Only color text matching the given regex pattern. By default excludes fasta and fastq header lines."
    )]
    regex: String,

    // Operations.
    #[arg(
        short('T'),
        long,
        help = "Transpose, i.e. swap columns and rows. \
        May be an improvement for scrolling long sequences. \
        Non-streaming."
    )]
    transpose: bool,

    #[arg(
        short('c'),
        long("consensus"),
        value_name("STYLE"),
        help = "Compute the consensus sequence and indicate it in each sequence by \"bold\", \"underline\", or a color. \
        Currently unaffected by the regex and min length options. \
        Non-streaming."
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
        value_name("STYLE"),
        help = "Opposite of -c/--consensus. \
        Highlight mutations/deviations from consensus."
    )]
    not_consensus: Option<String>,

    // Misc options.
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

    // Read colorschemes

    let mut colors_bg: HashMap<char, Color> = match args.colorscheme {
        None => schemes.get("shapely_aa").unwrap().clone(),
        Some(scheme_names) => {
            let mut colors: HashMap<char, Color> = HashMap::new();
            for scheme_name in scheme_names {
                // Ignore empty string, which allows for disabling bg coloring all together.
                if scheme_name != "" {
                    match schemes.get(&scheme_name) {
                        Some(_colors) => colors.extend(_colors),
                        None => colors.extend(
                            colorschemes::read_colorscheme(&scheme_name)
                                .expect("Colorscheme not understood"),
                        ),
                    };
                }
            }
            colors
        }
    };

    let mut colors_fg: HashMap<char, Color> = match args.foreground {
        None => {
            let mut colors: HashMap<char, Color> = HashMap::new();
            colors.insert('-', Color::Rgb(128, 128, 128));
            // Make text legible by using dark text with light bg, and light text with dark bg.
            // We can either explicitly set the text fg to black and white, or use inversion to use the
            // terminal colours. Here we wanted to do the latter but it breaks the pager.
            for (c, col) in colors_bg.iter() {
                if is_light(*col) {
                    colors.insert(*c, Black);
                } else {
                    colors.insert(*c, White);
                }
            }
            colors
        }
        Some(scheme_names) => {
            let mut colors: HashMap<char, Color> = HashMap::new();
            for scheme_name in scheme_names {
                // Ignore empty string, which allows for disabling bg coloring all together.
                if scheme_name != "" {
                    match schemes.get(&scheme_name) {
                        Some(_colors) => colors.extend(_colors),
                        None => colors.extend(
                            colorschemes::read_colorscheme(&scheme_name)
                                .expect("Colorscheme not understood"),
                        ),
                    };
                }
            }
            colors
        }
    };

    match args.invisible {
        None => {}
        Some(invisible) => {
            if invisible == "." {
                for (&c, col) in colors_bg.iter() {
                    colors_fg.insert(c, col.to_owned());
                }
            } else if invisible.starts_with("^") {
                let visible = &invisible[1..];
                for (&c, col) in colors_bg.iter() {
                    if !visible.contains(c) {
                        colors_fg.insert(c, col.to_owned());
                    }
                }
            } else {
                for c in invisible.chars() {
                    match colors_bg.get(&c) {
                        Some(&col) => colors_fg.insert(c, col),
                        None => panic!("Invisible only supported for char with a bg color."),
                    };
                }
            }
        }
    }

    // Use the highest fidelity ansi colors that the current terminal emulator supports.
    if anstyle_query::truecolor() {
    } else if anstyle_query::term_supports_ansi_color() {
        for col in colors_bg.values_mut() {
            *col = Fixed(ansi256(*col));
        }
        for col in colors_fg.values_mut() {
            *col = Fixed(ansi256(*col));
        }
    } else if anstyle_query::term_supports_color() {
        unimplemented!()
    } else {
        unimplemented!()
    }

    // Combine fg and bg. A char may have fg, bg, or both.
    let mut styles = HashMap::new();
    for (&c, &col) in colors_bg.iter() {
        styles.insert(c, col.background());
    }
    for (&c, &col) in colors_fg.iter() {
        match styles.get(&c) {
            None => {
                styles.insert(c, col.foreground());
            }
            Some(&style) => {
                styles.insert(c, style.fg(col));
            }
        }
    }

    let mut regexes = vec![];

    match args.regex.as_str() {
        ".*" => {}
        s_re => regexes.push(Regex::new(s_re).expect("Regex not understood.")),
    };

    match args.min_seq_length {
        None => {}
        Some(min_seq_length) => {
            // Build regex of min length of matches taken from the colorscheme alphabet.
            let mut alphabet = Vec::new();
            for c in styles.keys() {
                // characters with special meaning inside regex [...]
                if "^[]-".contains(*c) {
                    alphabet.push('\\'); // Escape them.
                }
                alphabet.push(*c);
            }
            let alphabet: String = alphabet.iter().collect();
            regexes.push(Regex::new(format!("[{alphabet}]{{{min_seq_length},}}").as_str()).unwrap())
        }
    };

    let comp_consensus = args.consensus.is_some();

    if !args.transpose && !comp_consensus {
        // Streaming.
        let lines = read_lines(args.files)?;

        match regexes.len() {
            0 => {
                // No filters, simply color every line.
                for line in lines {
                    print_ansi(&styles, &line);
                    println!();
                }
            }
            1 => {
                let re = &regexes[0];
                for line in lines {
                    let mut i = 0;
                    for m in re.find_iter(&line) {
                        print!("{}", &line[i..m.start()]);
                        print_ansi(&styles, m.as_str());
                        i = m.end();
                    }
                    println!("{}", &line[i..]);
                }
            }
            2 => {
                // Boolean logic: color only if both regex filters says yes.
                let re0 = &regexes[0];
                let re1 = &regexes[1];
                for line in lines {
                    let mut i = 0;
                    for m0 in re0.find_iter(&line) {
                        print!("{}", &line[i..m0.start()]);
                        i = m0.start();
                        for m1 in re1.find_iter(m0.as_str()) {
                            print!("{}", &line[i..m1.start()]);
                            print_ansi(&styles, m1.as_str());
                            i = m1.end();
                        }
                        print!("{}", &line[i..m0.end()]);
                        i = m0.end();
                    }
                    println!("{}", &line[i..]);
                }
            }
            _ => unimplemented!(), // Unreachable
        };
    } else {
        // Not streaming.
        // First read input into memory.
        let (lines, max_line) = inout::read_lines_max(args.files)?;

        // Gather styles according to each char in each line.
        let mut lines_painted: Vec<Vec<Painted<char>>> = Vec::with_capacity(lines.len());

        match regexes.len() {
            0 => {
                for line in lines {
                    lines_painted.push(to_painted(&styles, &line).collect());
                }
            }
            1 => {
                let re = &regexes[0];
                for line in lines {
                    let mut line_painted: Vec<Painted<char>> = Vec::with_capacity(line.len());
                    let mut i = 0;
                    for m in re.find_iter(&line) {
                        for c in line[i..m.start()].chars() {
                            line_painted.push(Paint::new(c));
                        }
                        line_painted.extend(to_painted(&styles, m.as_str()));
                        i = m.end();
                    }
                    for c in line[i..].chars() {
                        line_painted.push(Paint::new(c));
                    }
                    lines_painted.push(line_painted);
                }
            }
            2 => {
                // Boolean logic: color only if both regex filters says yes.
                let re0 = &regexes[0];
                let re1 = &regexes[1];
                for line in lines {
                    let mut line_painted: Vec<Painted<char>> = Vec::with_capacity(line.len());
                    let mut i = 0;
                    for m0 in re0.find_iter(&line) {
                        for c in line[i..m0.start()].chars() {
                            line_painted.push(Paint::new(c));
                        }
                        i = m0.start();
                        for m1 in re1.find_iter(m0.as_str()) {
                            for c in line[i..m1.start()].chars() {
                                line_painted.push(Paint::new(c));
                            }
                            line_painted.extend(to_painted(&styles, m1.as_str()));
                            i = m1.end();
                        }
                        for c in line[i..m0.end()].chars() {
                            line_painted.push(Paint::new(c));
                        }
                        i = m0.end();
                    }
                    for c in line[i..].chars() {
                        line_painted.push(Paint::new(c));
                    }
                    lines_painted.push(line_painted);
                }
            }
            _ => unimplemented!(), // Unreachable
        }

        if comp_consensus {
            // Count char occurrences.
            let mut letter_counts: Vec<HashMap<char, i32>> = Vec::with_capacity(max_line);
            for _ in 0..max_line {
                letter_counts.push(HashMap::new());
            }
            for painted_line in &lines_painted {
                for (i, painted) in painted_line.iter().enumerate() {
                    let c = painted.value;
                    // Only include what is styled, which will effectively apply the regex etc. 
                    // filters to consensus comp.
                    if is_styled(painted) {
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

            // Collect references to all consensus chars.
            let mut consensus_chars = vec![];
            for painted_line in &mut lines_painted {
                for (i, painted) in painted_line.iter_mut().enumerate() {
                    match consensus[i] {
                        None => {}
                        Some(_consensus) => {
                            if _consensus == painted.value && is_styled(painted) {
                                consensus_chars.push(painted);
                            }
                        }
                    }
                }
            }

            // Apply either an attribute or bg color to letters matching the consensus.
            match args.consensus {
                None => None, // unreachable
                Some(s_style) => Some(match s_style.as_str() {
                    "bold" => {
                        for painted in consensus_chars {
                            painted.style = painted.style.bold();
                        }
                    }
                    "underline" => {
                        for painted in consensus_chars {
                            painted.style = painted.style.underline();
                        }
                    }
                    color => {
                        let col = parse_color(color).expect(color);
                        for painted in consensus_chars {
                            painted.style = painted.style.bg(col);
                        }
                    }
                }),
            };
        }

        if !args.transpose {
            for painted_line in &lines_painted {
                for painted in painted_line {
                    print!("{}", painted);
                }
                println!();
            }
        } else {
            // Transpose.
            for j in 0..max_line {
                for painted_line in &lines_painted {
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
