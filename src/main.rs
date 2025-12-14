use anyhow::Result;
use clap::Parser;
use regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::process::{exit, Child, Command, Stdio};
use std::{collections::HashMap, env, vec};

use yansi::Color::{self, *};

mod ansi_colors;
mod bio;
mod colorschemes;
mod consensus;
mod inout;

use crate::ansi_colors::{ansi16, ansi256, ansi_byte, is_light, to_painted, write_ansi, write_line, Char};
use crate::inout::read_lines;

/// Pager mode configuration.
enum PagingMode {
    /// Always use a pager (without auto-quit).
    Always,
    /// Never use a pager.
    Never,
    /// Use a pager if stdout is a TTY, with auto-quit if content fits on screen.
    Auto,
}

impl PagingMode {
    fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "always" => Ok(PagingMode::Always),
            "never" => Ok(PagingMode::Never),
            "auto" => Ok(PagingMode::Auto),
            _ => Err(anyhow::anyhow!("Invalid paging value: '{}'. Use 'auto', 'never', or 'always'.", s)),
        }
    }

    /// Determine if we should use a pager.
    fn should_page(&self) -> bool {
        match self {
            PagingMode::Always => true,
            PagingMode::Never => false,
            PagingMode::Auto => io::stdout().is_terminal(),
        }
    }
}

/// Spawn a pager process and return it along with its stdin for writing.
/// In auto mode, passes flags to make less quit if content fits on one screen.
fn spawn_pager(auto_quit: bool) -> Option<Child> {
    let pager = env::var("PAGER").unwrap_or_else(|_| "less".to_string());

    // Parse pager command (may include arguments like "less -R")
    let mut parts = pager.split_whitespace();
    let cmd = parts.next()?;
    let args: Vec<&str> = parts.collect();

    let mut command = Command::new(cmd);
    command.args(&args);

    // If using less, ensure -R is set for ANSI color support
    if cmd == "less" {
        if !args.iter().any(|a| a.contains("-R") || a.contains("--RAW-CONTROL-CHARS")) {
            command.arg("-R"); // Interpret ANSI color sequences
        }
        // Add other defaults only if no arguments were provided
        if args.is_empty() {
            command.arg("-S"); // Chop long lines (horizontal scroll instead of wrap)
            command.arg("-K"); // Quit on Ctrl-C
            if auto_quit {
                command.arg("-F"); // Quit if content fits on one screen
                command.arg("-X"); // Don't clear screen (prevents flicker with -F)
            }
        }
    }

    // Set LESSCHARSET for proper UTF-8 handling
    command.env("LESSCHARSET", "UTF-8");

    command
        .stdin(Stdio::piped())
        .spawn()
        .ok()
}

#[derive(Debug, Parser)]
#[command(
    author = "Christian Madsen",
    version = "0.1.0",
    about = "Colourise sequences of characters based on the characters.",
    long_about = "Colourise biological sequences (amino acids, DNA, and RNA). \
    Useful for viewing fasta files, sequence alignments, CSV, TSV, and other text files. \
    A simple commandline tool like `cat`, which may be useful for colourising \
    sequence of characters in general.",
    after_help = "PERFORMANCE: For improved pager scrolling with long lines, \
    use --colors=256 or --colors=16 (smallest output). Transposing with -T may also help in some cases."
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
        long("bg"),
        value_name("COLORSCHEME"),
        help = "Name of predefined colorscheme or file with custom colorscheme to control background color for each given character. \
        Flag can be specified multiple times where \
        definitions in subsequent color schemes take precedence over previous. \
        Use -l/--list-schemes to get list of available colorschemes. \
        Colorscheme file format: each line contains a character and a color separated by a delimiter. The delimiter can be tab, comma, semicolon, etc. \
        The color can be a color name, hex, or integer triplet delimited by spaces or commas."
    )]
    background: Option<Vec<String>>,

    #[arg(
        short('S'),
        long("fg"),
        value_name("COLORSCHEME"),
        help = "The same as -s/--bg, except controls character foreground instead of background colors (the character itself). \
        By default each character is either white or black depending on lightness of their background color, while gaps are gray. \
        Foreground color is also modified by -i/--invisible."
    )]
    foreground: Option<Vec<String>>,

    #[arg(
        short('a'),
        long,
        help = "Specify the alphabet. Affects -c/--consensus. Only affects colouring if -m/--min is supplied. \
        Valid arg is a path of a file containing the alphabet, or one of the valid keywords: \
        \"dna\", \"rna\", \"nucl\", \"aa\", \"aax\", \"all\", or any of these followed by \" no gap\". \
        \"aax\" is amino acid residues including BZX. \
        Default is no alphabet, which means anything matching -r/--regex and -m/--min will be counted for -c/--consensus."
    )]
    alphabet: Option<String>,

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
        value_name("CHARS"),
        help = "Hide letter codes by showing text foreground color the same as background color (-s/--bg). \
        Argument should be which characters to make invisible. \
        Use \".\" for all characters given to -s/--bg. \
        If prefixed by \"^\", then it's reversed. \
        Takes precedence over -S/--fg."
    )]
    invisible: Option<String>,

    // Options controlling what to color.
    #[arg(
        short('m'),
        long("min"),
        value_name("LENGTH"),
        help = "Minimum sequence length to color. \
        If -a/--alphabet is supplied then minimum length of characters from the chosen alphabet, otherwise any char. \
        Useful to avoid highlighting non-sequence text e.g. in a table file."
    )]
    min_seq_length: Option<u32>,

    #[arg(
        short('r'),
        long,
        default_value = "^[^>@+].*",
        value_name("PATTERN"),
        help = "Only color text matching the given regex pattern. \
        By default excludes fasta and fastq header lines. \
        Useful for only highlighting matches of a restriction enzyme, binding site etc."
    )]
    regex: String,

    // Operations.

    #[arg(
        short('c'),
        long("consensus"),
        value_name("STYLE"),
        help = "Compute the consensus sequence and indicate it in each sequence by \"bold\", \"underline\", or a color. \
        Ties: no letter is highlighted. \
        Affected by options -r/--regex, -m/--min, and -a/--alphabet. \
        Non-streaming."
    )]
    consensus: Option<String>,

    #[arg(
        short('C'),
        long("mut"),
        value_name("STYLE"),
        help = "Opposite of -c/--consensus. \
        Highlight mutations/deviations from consensus. \
        Affected by options -r/--regex, -m/--min, and -a/--alphabet. \
        Non-streaming."
    )]
    mutations: Option<String>,

    #[arg(
        short('T'),
        long,
        help = "Transpose, i.e. swap columns and rows. \
        Only sequence lines (colored) are transposed; other lines (e.g. fasta headers) are printed normally first. \
        May be useful for scrolling long sequences. \
        Non-streaming."
    )]
    transpose: bool,

    // Misc options.
    #[arg(
        short('l'),
        long("list-schemes"),
        help = "List available colorschemes."
    )]
    list_colorschemes: bool,

    #[arg(
        short('p'),
        long("paging"),
        value_name("WHEN"),
        env("SEQCOL_PAGING"),
        default_value = "auto",
        help = "When to use a pager. \
        \"auto\" (default): use pager if stdout is a terminal, quit automatically if content fits on screen. \
        \"always\": always use pager. \
        \"never\": never use pager. \
        The pager command is taken from $PAGER, defaulting to \"less -RSKFX\" (auto) or \"less -RSK\" (always). \
        If $PAGER is set to less with custom args, -R is added automatically for ANSI color support."
    )]
    paging: String,

    #[arg(
        short('k'),
        long("colors"),
        value_name("MODE"),
        value_parser(["16", "256", "true"]),
        help = "Color output mode. \
        \"true\" (default): truecolor (24-bit). \
        \"256\": 256-color palette. \
        \"16\": basic 16 ANSI colors (smallest output). \
        Without this flag, mode is auto-detected from COLORTERM/TERM environment."
    )]
    colors: Option<String>,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        // Silently exit on broken pipe (e.g., when pager closes early).
        if let Some(io_err) = e.downcast_ref::<io::Error>()
            && io_err.kind() == io::ErrorKind::BrokenPipe
        {
            std::process::exit(0);
        }
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// Print colorscheme list with aligned descriptions and word wrapping.
fn print_colorscheme_list(schemes: &[(String, String)]) {
    // Find the longest name for alignment
    let max_name_len = schemes.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let indent = " ".repeat(max_name_len + 2);

    let options = textwrap::Options::with_termwidth()
        .initial_indent("")
        .subsequent_indent(&indent);

    for (name, description) in schemes {
        if description.is_empty() {
            println!("{}", name);
        } else {
            let line = format!("{:width$}  {}", name, description, width = max_name_len);
            println!("{}", textwrap::fill(&line, &options));
        }
    }
}

fn run(args: Args) -> Result<()> {
    if args.list_colorschemes {
        let schemes = colorschemes::get_colorscheme_names();
        print_colorscheme_list(&schemes);
        exit(0)
    }

    let schemes = colorschemes::load_colorschemes();

    // Read colorschemes

    let mut colors_bg: HashMap<char, Color> = match args.background {
        None => HashMap::new(),
        Some(scheme_names) => {
            let mut colors: HashMap<char, Color> = HashMap::new();
            for scheme_name in scheme_names {
                // Ignore empty string, which allows for disabling bg coloring all together.
                if !scheme_name.is_empty() {
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
                if !scheme_name.is_empty() {
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
            } else if let Some(visible) = invisible.strip_prefix('^') {
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
    // --colors flag takes priority, then fall back to COLORTERM/TERM env vars.
    let use_16 = args.colors.as_deref() == Some("16")
        || (args.colors.is_none() && env::var("TERM").ok().as_deref() == Some("dumb"));
    let use_256 = args.colors.as_deref() == Some("256")
        || (!use_16 && args.colors.is_none() && !anstyle_query::truecolor());

    if use_16 {
        for col in colors_bg.values_mut() {
            *col = ansi16(*col);
        }
        for col in colors_fg.values_mut() {
            *col = ansi16(*col);
        }
    } else if use_256 {
        for col in colors_bg.values_mut() {
            *col = Fixed(ansi256(*col));
        }
        for col in colors_fg.values_mut() {
            *col = Fixed(ansi256(*col));
        }
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

    let comp_consensus = args.consensus.is_some() || args.mutations.is_some();

    // Read alphabet arg if relevant.
    let alphabet: Option<HashSet<char>> = if comp_consensus || args.min_seq_length.is_some() {
        match args.alphabet {
            None => None,
            Some(arg) => {
                // Detect if filename or a keyword by trying to open.
                let s_alphabet = match File::open(&arg) {
                    Err(_) => {
                        let arg = arg.to_lowercase().replace(" ", "");
                        match bio::ALPHABETS.get(&arg) {
                            None => panic!("Alphabet keyword not understood."),
                            Some(alphabet) => alphabet.to_string(),
                        }
                    }
                    Ok(mut file) => {
                        let mut alphabet = String::new();
                        file.read_to_string(&mut alphabet)?;
                        alphabet
                    }
                };
                let mut alphabet = HashSet::with_capacity(s_alphabet.len());
                for c in s_alphabet.chars() {
                    alphabet.insert(c);
                }
                Some(alphabet)
            }
        }
    } else {
        None
    };

    let mut regexes = vec![];

    match args.regex.as_str() {
        ".*" => {}
        s_re => regexes.push(Regex::new(s_re).expect("Regex not understood.")),
    };

    match args.min_seq_length {
        None => {}
        Some(min_seq_length) => {
            // Build regex of min length of matches taken from the alphabet if one is supplied.
            let re_alphabet = match &alphabet {
                Some(_alphabet) => {
                    let mut re_alphabet = String::with_capacity(_alphabet.len()+2);
                    re_alphabet.push('[');
                    for c in _alphabet {
                        // characters with special meaning inside regex [...]
                        if "^[]-".contains(*c) {
                            re_alphabet.push('\\'); // Escape them.
                        }
                        re_alphabet.push(*c);
                    }
                    re_alphabet.push(']');
                    re_alphabet
                },
                None => ".".to_string()
            };
            regexes.push(Regex::new(format!("{re_alphabet}{{{min_seq_length},}}").as_str()).unwrap())
        }
    };

    // Set up output destination (stdout or pager)
    let paging_mode = PagingMode::parse(&args.paging)?;
    let auto_quit = matches!(paging_mode, PagingMode::Auto);
    let mut pager_child = if paging_mode.should_page() {
        spawn_pager(auto_quit)
    } else {
        None
    };

    // Get a writer - either pager stdin or stdout
    let mut stdout_lock = io::stdout().lock();
    let mut pager_stdin: Option<std::process::ChildStdin> = None;
    let output: &mut dyn Write = match &mut pager_child {
        Some(child) if child.stdin.is_some() => {
            pager_stdin = child.stdin.take();
            pager_stdin.as_mut().unwrap()
        }
        _ => &mut stdout_lock,
    };

    let newline = ansi_byte('\n');
    let space = ansi_byte(' ');

    if !args.transpose && !comp_consensus {
        // Streaming.
        let lines = read_lines(args.files)?;

        match regexes.len() {
            0 => {
                // No filters, simply color every line.
                for line in lines {
                    write_ansi(output, &styles, &line)?;
                    output.write_all(&newline)?;
                }
            }
            1 => {
                let re = &regexes[0];
                for line in lines {
                    let mut i = 0;
                    for m in re.find_iter(&line) {
                        output.write_all(&line.as_bytes()[i..m.start()])?;
                        write_ansi(output, &styles, m.as_str())?;
                        i = m.end();
                    }
                    output.write_all(&line.as_bytes()[i..])?;
                    output.write_all(&newline)?;
                }
            }
            2 => {
                // Boolean logic: color only if both regex filters says yes.
                let re0 = &regexes[0];
                let re1 = &regexes[1];
                for line in lines {
                    let mut i = 0;
                    for m0 in re0.find_iter(&line) {
                        output.write_all(&line.as_bytes()[i..m0.start()])?;
                        i = m0.start();
                        for m1 in re1.find_iter(m0.as_str()) {
                            output.write_all(&line.as_bytes()[i..m1.start()])?;
                            write_ansi(output, &styles, m1.as_str())?;
                            i = m1.end();
                        }
                        output.write_all(&line.as_bytes()[i..m0.end()])?;
                        i = m0.end();
                    }
                    output.write_all(&line.as_bytes()[i..])?;
                    output.write_all(&newline)?;
                }
            }
            _ => unimplemented!(), // Unreachable
        };
    } else {
        // Not streaming.
        // First read input into memory.
        let (lines, max_line) = inout::read_lines_max(args.files)?;

        // Gather styles according to each char in each line.
        let mut lines_painted: Vec<Vec<Char>> = Vec::with_capacity(lines.len());

        match regexes.len() {
            0 => {
                for line in lines {
                    lines_painted.push(to_painted(&styles, &line).collect());
                }
            }
            1 => {
                let re = &regexes[0];
                for line in lines {
                    let mut line_painted: Vec<Char> = Vec::with_capacity(line.len());
                    let mut i = 0;
                    for m in re.find_iter(&line) {
                        for c in line[i..m.start()].chars() {
                            line_painted.push(Char::Unstyled(c));
                        }
                        line_painted.extend(to_painted(&styles, m.as_str()));
                        i = m.end();
                    }
                    for c in line[i..].chars() {
                        line_painted.push(Char::Unstyled(c));
                    }
                    lines_painted.push(line_painted);
                }
            }
            2 => {
                // Boolean logic: color only if both regex filters says yes.
                let re0 = &regexes[0];
                let re1 = &regexes[1];
                for line in lines {
                    let mut line_painted: Vec<Char> = Vec::with_capacity(line.len());
                    let mut i = 0;
                    for m0 in re0.find_iter(&line) {
                        for c in line[i..m0.start()].chars() {
                            line_painted.push(Char::Unstyled(c));
                        }
                        i = m0.start();
                        for m1 in re1.find_iter(m0.as_str()) {
                            for c in line[i..m1.start()].chars() {
                                line_painted.push(Char::Unstyled(c));
                            }
                            line_painted.extend(to_painted(&styles, m1.as_str()));
                            i = m1.end();
                        }
                        for c in line[i..m0.end()].chars() {
                            line_painted.push(Char::Unstyled(c));
                        }
                        i = m0.end();
                    }
                    for c in line[i..].chars() {
                        line_painted.push(Char::Unstyled(c));
                    }
                    lines_painted.push(line_painted);
                }
            }
            _ => unimplemented!(), // Unreachable
        }

        if comp_consensus {
            let highlight_consensus = args.consensus.is_some();
            let style = args.consensus.as_ref().or(args.mutations.as_ref()).unwrap();
            consensus::highlight_consensus_or_mutations(
                &mut lines_painted,
                max_line,
                &alphabet,
                highlight_consensus,
                style,
            );
        }

        if !args.transpose {
            for painted_line in &lines_painted {
                write_line(output, painted_line)?;
                output.write_all(&newline)?;
            }
        } else {
            // Transpose: only transpose lines containing styled (sequence) characters.
            // Non-sequence lines are output normally before the transposed block.
            let mut seq_lines: Vec<&Vec<Char>> = Vec::new();
            let mut seq_max_len = 0;

            for painted_line in &lines_painted {
                let has_styled = painted_line.iter().any(|c| matches!(c, Char::Styled(_)));
                if has_styled {
                    seq_lines.push(painted_line);
                    seq_max_len = seq_max_len.max(painted_line.len());
                } else {
                    // Output non-sequence lines normally.
                    write_line(output, painted_line)?;
                    output.write_all(&newline)?;
                }
            }

            // Transpose the sequence lines.
            for j in 0..seq_max_len {
                for seq_line in &seq_lines {
                    match seq_line.get(j) {
                        None => output.write_all(&space)?,
                        Some(ch) => ch.write(output)?,
                    };
                }
                output.write_all(&newline)?;
            }
        }
    }

    // Flush output
    output.flush()?;

    // Drop the pager stdin to signal EOF, then wait for pager
    drop(pager_stdin);
    if let Some(mut child) = pager_child {
        let _ = child.wait();
    }

    Ok(())
}
