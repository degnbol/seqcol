use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use regex::bytes::Regex;
use std::collections::HashMap;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use yansi::Color::{self, *};

use seqcol::ansi_colors::{combine, is_light, reduce_depth, ColorDepth};
use seqcol::{bio, colorschemes, inout, matches, pager, Config, Highlight};

/// Pager mode configuration.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum PagingMode {
    /// Always use a pager (without auto-quit).
    Always,
    /// Never use a pager.
    Never,
    /// Use a pager if stdout is a TTY, with auto-quit if content fits on screen.
    Auto,
}

impl PagingMode {
    /// Determine if we should use a pager.
    fn should_page(&self) -> bool {
        match self {
            PagingMode::Always => true,
            PagingMode::Never => false,
            PagingMode::Auto => io::stdout().is_terminal(),
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    author = "Christian Madsen",
    version,
    about = "Colourise sequences of characters based on the characters.",
    long_about = "Colourise biological sequences (amino acids, DNA, and RNA). \
    Useful for viewing fasta files, sequence alignments, CSV, TSV, and other text files. \
    A simple commandline tool like `cat`, which may be useful for colourising \
    sequence of characters in general.",
    after_help = "Performance: For improved pager scrolling with long lines, \
    use --colors=256 or --colors=16 (smallest output). Transposing with -T may also help for long sequences."
)]
struct Args {
    // Input file(s)
    #[arg(
        value_name = "FILE",
        default_value = "-",
        help = "Text containing sequences, plain or gzip-compressed. Default is reading stdin."
    )]
    files: Vec<String>,

    // Options controlling how to color.
    #[arg(
        short('s'),
        long("bg"),
        value_name("COLORSCHEME"),
        help = "Name of predefined colorscheme or file with custom colorscheme to control background color for each given character. \
        Builtin schemes are named with _aa or _nucl suffix (e.g. taylor_aa, taylor_nucl). \
        You can use the base name (e.g. \"taylor\") and the suffix will be auto-detected from sequence content. \
        Use -a/--alphabet to override auto-detection. \
        Flag can be specified multiple times where definitions in subsequent schemes take precedence. \
        Use -l/--list-schemes for available colorschemes. \
        Custom file format: each line has a character and color separated by tab/comma/etc. \
        Colors can be names, hex (#ff0000), or RGB (255 0 0)."
    )]
    background: Option<Vec<String>>,

    #[arg(
        short('S'),
        long("fg"),
        value_name("COLORSCHEME"),
        help = "Same as -s/--bg but controls foreground (text) color instead of background. \
        Supports base name auto-detection like -s/--bg. \
        Default: white/black text based on background lightness, gray for gaps. \
        Modified by -i/--invisible."
    )]
    foreground: Option<Vec<String>>,

    #[arg(
        short('a'),
        long,
        help = "Specify the alphabet. Affects -m/--min, -c/--consensus, and colorscheme auto-detection. \
        When set to \"dna\", \"rna\", or \"nucl\", colorschemes with base names resolve to _nucl suffix. \
        When set to \"aa\" or \"aax\", they resolve to _aa suffix. \
        Valid values: file path, or keywords \"dna\" (ACGTN), \"rna\" (ACGUN), \"nucl\" (ACGTUN), \
        \"aa\" (20 amino acids and *), \"aax\" (aa and ambiguous BZX), \"all\" (A-Z and *). \
        Each keyword includes the gap -, except with the suffix \" no gap\". \
        Default: no alphabet, and the sequence type is detected from the text to color."
    )]
    alphabet: Option<String>,

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
        A sequence is a run of characters from -a/--alphabet, or from \"all\" without -a. \
        Useful to avoid highlighting non-sequence text e.g. in a table file."
    )]
    min_seq_length: Option<u32>,

    #[arg(
        short('r'),
        long,
        default_value = "(?-u)^[^>@+].*",
        value_name("PATTERN"),
        help = "Only color text matching the given regex pattern. \
        By default excludes fasta and fastq header lines. \
        Useful for only highlighting matches of a restriction enzyme, binding site etc. \
        With -m/--min, only text matched by both is colored. \
        Patterns match UTF-8 text, so . does not match an invalid byte. Prefix (?-u) to match bytes."
    )]
    regex: String,

    // Operations.

    #[arg(
        short('c'),
        long("consensus"),
        value_name("STYLE"),
        help = "Compute the consensus sequence and indicate it in each sequence by a style: \
        \"bold\", \"underline\", \"italic\", a background color, or a combination (e.g. \"bold red\"). \
        Ties: no letter is highlighted. \
        Does not support sequence spanning multiple lines. \
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
        Does not support sequence spanning multiple lines. \
        Affected by options -r/--regex, -m/--min, and -a/--alphabet. \
        Non-streaming."
    )]
    mutations: Option<String>,

    #[arg(
        short('T'),
        long,
        help = "Transpose, i.e. swap columns and rows. \
        Only sequence lines (colored) are transposed; other lines (e.g. fasta headers) are printed normally first. \
        Does not support sequence spanning multiple lines. \
        May be useful for scrolling long sequences. \
        Non-streaming."
    )]
    transpose: bool,

    // Misc options.
    #[arg(
        short('p'),
        long("paging"),
        value_name("WHEN"),
        env("SEQCOL_PAGING"),
        default_value = "auto",
        ignore_case = true,
        hide_possible_values = true,
        help = "When to use a pager. \
        \"auto\" (default): if stdout is a terminal and the content does not fit on one screen. \
        \"always\": always. \
        \"never\": never. \
        Default pager is `less -SK~#8RXL --mouse --wheel-lines=5`. Overridden by $LESS and $PAGER."
    )]
    paging: PagingMode,

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

    #[arg(
        short('l'),
        long("list-schemes"),
        help = "List available colorschemes."
    )]
    list_colorschemes: bool,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(code) => code,
        // Silently exit on a closed output from -l.
        Err(e) if e.downcast_ref::<io::Error>().is_some_and(|io_err| io_err.kind() == io::ErrorKind::BrokenPipe) => {
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Convert command line arguments to a coloring configuration.
///
/// Reads the start of `args.files` to detect the sequence type, unless `args.alphabet` names one.
/// Without `--colors`, reads `TERM` and `COLORTERM` to select the color depth.
///
/// # Arguments
/// * `args` - Parsed command line arguments
///
/// # Returns
/// Coloring configuration, or the error from an invalid `-c`/`-C` style or from reading an alphabet file.
///
/// # Panics
/// On an unknown colorscheme or alphabet, an invalid regex, or an invisible character without a background color.
fn config(args: &Args) -> Result<Config> {
    let schemes = colorschemes::load_colorschemes();

    let alphabet = args.alphabet.as_deref().map(bio::read_alphabet).transpose()?;

    let mut regexes = vec![];
    if args.regex != ".*" {
        regexes.push(Regex::new(&args.regex).expect("Regex not understood."));
    }
    if let Some(min_seq_length) = args.min_seq_length {
        let seq_alphabet = match &alphabet {
            Some(alphabet) => alphabet,
            None => &bio::ALPHABETS["all"].chars().collect(),
        };
        regexes.push(matches::min_length_regex(seq_alphabet, min_seq_length as usize));
    }

    // Sequence type of -a/--alphabet, else detected from the text to color.
    let seq_type: Option<bio::SeqType> = args
        .alphabet
        .as_ref()
        .and_then(|a| bio::SeqType::from_alphabet(a))
        .or_else(|| bio::detect_seq_type_from_files(&args.files, &regexes));

    let colors_bg = colorschemes::read_schemes(args.background.as_deref().unwrap_or_default(), seq_type, &schemes);

    let mut colors_fg: HashMap<char, Color> = match &args.foreground {
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
        Some(scheme_names) => colorschemes::read_schemes(scheme_names, seq_type, &schemes),
    };

    match &args.invisible {
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
    let depth = match args.colors.as_deref() {
        Some("16") => ColorDepth::Ansi16,
        Some("256") => ColorDepth::Fixed256,
        Some(_) => ColorDepth::True,
        None if env::var("TERM").ok().as_deref() == Some("dumb") => ColorDepth::Ansi16,
        None if !anstyle_query::truecolor() => ColorDepth::Fixed256,
        None => ColorDepth::True,
    };
    let styles = combine(&colors_fg, &colors_bg)
        .into_iter()
        .map(|(c, style)| (c, reduce_depth(style, depth)))
        .collect();

    let highlight_style = args
        .consensus
        .as_ref()
        .or(args.mutations.as_ref())
        .map(|text| colorschemes::parse_style(text).map_err(|e| anyhow!("{text}: {e}")))
        .transpose()?
        .map(|style| reduce_depth(style, depth));

    Ok(Config {
        styles,
        regexes,
        transpose: args.transpose,
        highlight: highlight_style.map(|style| Highlight {
            consensus: args.consensus.is_some(),
            style,
            alphabet,
        }),
    })
}

fn run(args: Args) -> Result<ExitCode> {
    if args.list_colorschemes {
        let schemes = colorschemes::get_colorscheme_names();
        colorschemes::write_colorscheme_list(&mut io::stdout().lock(), &schemes)?;
        return Ok(ExitCode::SUCCESS);
    }

    let config = config(&args)?;

    // Handle non-existent files like `cat`: report files that fail to open, read the rest, and exit with failure.
    // Open before the pager starts, so errors are not hidden by pager output.
    let readers: Vec<_> = args.files.iter()
        .filter_map(|f| inout::open(f).inspect_err(|e| eprintln!("{f}: {e}")).ok().map(|r| (f.clone(), r)))
        .collect();
    let all_opened = readers.len() == args.files.len();

    // Set up output destination (stdout or pager)
    let mut pager_child = if args.paging.should_page() {
        pager::spawn_pager(matches!(args.paging, PagingMode::Auto))
    } else {
        None
    };

    // Get a writer: either stdout or pager stdin
    let mut stdout_lock = io::stdout().lock();
    let mut pager_stdin: Option<std::process::ChildStdin> = None;
    let output: &mut dyn Write = match &mut pager_child {
        Some(child) if child.stdin.is_some() => {
            pager_stdin = child.stdin.take();
            pager_stdin.as_mut().unwrap()
        }
        _ => &mut stdout_lock,
    };

    let result = seqcol::run(readers, output, &config);

    // Drop the pager stdin to signal EOF, then wait for pager
    drop(pager_stdin);
    if let Some(mut child) = pager_child {
        let _ = child.wait();
    }

    // Report read errors after the pager closes, so they stay visible.
    let read_errors = result?;
    for e in &read_errors {
        eprintln!("{e}");
    }
    Ok(if all_opened && read_errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
