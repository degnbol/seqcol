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
    Paint, Painted,
};

mod ansi_colors;
mod colorschemes;
mod inout;

use crate::ansi_colors::{ansi256,is_light,print_ansi,to_painted};


#[derive(Debug, Parser)]
#[command(
    author="Christian Madsen",
    version="0.1.0",
    about="Colourise sequences of characters based on the characters.",
    long_about="Colourise biological sequences (amino acids, DNA, and RNA). \
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

    #[arg(
        short('F'),
        long("no-fasta"),
        help = "By default lines starting with '>' are not colored."
    )]
    no_fasta_check: bool,

    #[arg(short('m'), long("min"), help = "Minimum sequence length to color.")]
    min_seq_length: Option<u32>,

    #[arg(
        short('r'),
        long,
        help = "Only color sequences that match the given regex."
    )]
    regex: Option<String>,

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
        },
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

    let re = match args.regex {
        None => None,
        Some(regex) => Some(Regex::new(regex.as_str()).expect("Regex not understood.")),
    };

    let re = match args.min_seq_length {
        None => re,
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
                                    print_ansi(&styles, &line);
                                    println!();
                                }
                                Some(_re) => {
                                    let mut i = 0;
                                    for m in _re.find_iter(&line) {
                                        print!("{}", &line[i..m.start()]);
                                        print_ansi(&styles, m.as_str());
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
                            painted_line.push(to_painted(&styles, c));
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
                                painted_line.push(to_painted(&styles, c));
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

