use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use regex::bytes::Regex;
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use yansi::Color::{self, *};
use yansi::Style;

use seqcol::ansi_colors::{combine, is_light, restrict, ColorDepth, Support};
use seqcol::{bio, colorschemes, inout, matches, pager, Config, Highlight, Styles};

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
    after_long_help = "Performance: For improved pager scrolling with long lines, \
    use --colors=256 or --colors=16 (smallest output). Transposing with -T may also help for long sequences."
)]
struct Args {
    // Input file(s)
    #[arg(
        value_name = "FILE",
        default_value = "-",
        hide_default_value = true,
        help = "Input files, plain or gzip-compressed. Default: stdin.",
        long_help = "Text containing sequences, plain or gzip-compressed. Default is reading stdin."
    )]
    files: Vec<String>,

    // Options controlling how to color.
    #[arg(
        short('s'),
        long("bg"),
        value_name("COLORSCHEME"),
        help = "Background colorscheme: a builtin name (see -l) or a file.",
        long_help = "Name of predefined colorscheme or file with custom colorscheme to control background color for each given character. \
        A scheme has colors for nucleotides, amino acids, or both. \
        Each run of text to color is classified as nucleotides or amino acids by its letters, and by the runs at the same place in other lines of its file. \
        Runs of a type without colors in any scheme are not colored. \
        So the output of a file can wait for up to 100 of its lines. \
        Use an -a/--alphabet keyword to give the type instead. \
        Flag can be specified multiple times where definitions in subsequent schemes take precedence. \
        Use -l/--list-schemes for available colorschemes. \
        Custom file format: each line has a character and color separated by tab/comma/etc. \
        Colors can be names, hex (#ff0000), or RGB (255 0 0). \
        The line \"#type: nucl\" or \"#type: aa\" starts the colors of that type; colors before it are for both types."
    )]
    background: Option<Vec<String>>,

    #[arg(
        short('S'),
        long("fg"),
        value_name("COLORSCHEME"),
        help = "Foreground colorscheme, like -s.",
        long_help = "Same as -s/--bg but controls foreground (text) color instead of background. \
        Colors each run by its type like -s/--bg. \
        Default: white/black text based on background lightness, gray for gaps. \
        Modified by -i/--invisible."
    )]
    foreground: Option<Vec<String>>,

    #[arg(
        short('a'),
        long,
        help = "Alphabet: dna, rna, nucl, aa, aax, all. Add +chars, remove ^chars.",
        long_help = "Specify the alphabet. Affects -m/--min, -c/--consensus, -C/--mut, and the sequence type of colorschemes. \
        \"dna\", \"rna\", and \"nucl\" color all text with the nucleotide colors of -s/--bg and -S/--fg. \
        \"aa\" and \"aax\" color all text with the amino acid colors. \
        With these keywords, output is not delayed. \
        Keywords: \"dna\" (ACGTN), \"rna\" (ACGUN), \"nucl\" (ACGTUN), \
        \"aa\" (20 amino acids and *), \"aax\" (aa and ambiguous BZX), \"all\" (A-Z and *). Each keyword includes the gaps (- and .). \
        +chars adds and ^chars removes characters, e.g. -a dna^-. for no gaps, or -a aa+X. \
        Removal has priority over addition. \
        seqcol ignores whitespace, and the case of the keyword. \
        The character after + or ^ is always literal, so ++ adds +. \
        Without a keyword, the edits apply to -f/--alphabet-file, or else to \"all\". \
        In zsh with EXTENDED_GLOB, quote a value with ^: -a 'dna^-.'. \
        Default: no alphabet. The letters of each run then give its sequence type."
    )]
    alphabet: Option<bio::AlphabetSpec>,

    #[arg(
        short('f'),
        long,
        value_name("FILE"),
        help = "Alphabet file. You can use -f more than once.",
        long_help = "File with alphabet characters. seqcol ignores whitespace. You can use -f more than once. \
        The alphabet is the union of the files and the -a/--alphabet keyword, with the -a/--alphabet edits."
    )]
    alphabet_file: Vec<String>,

    #[arg(
        short('i'),
        long,
        value_name("CHARS"),
        help = "Characters to hide: text color is the background color.",
        long_help = "Hide letter codes by showing text foreground color the same as background color (-s/--bg). \
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
        help = "Minimum length of a sequence run to color.",
        long_help = "Minimum sequence length to color. \
        A sequence is a run of characters of the alphabet from -a/--alphabet and -f/--alphabet-file, or of \"all\" without them. \
        Useful to avoid highlighting non-sequence text e.g. in a table file."
    )]
    min_seq_length: Option<u32>,

    #[arg(
        short('r'),
        long,
        default_value = "(?-u)^[^>@+].*",
        value_name("PATTERN"),
        help = "Color only text that matches this regex.",
        long_help = "Only color text matching the given regex pattern. \
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
        help = "Highlight the consensus with a style, e.g. \"bold\".",
        long_help = "Compute the consensus sequence and indicate it in each sequence by a style: \
        \"bold\", \"underline\", \"italic\", a background color, or a combination (e.g. \"bold red\"). \
        Ties: no letter is highlighted. \
        Does not support sequence spanning multiple lines. \
        Options -r/--regex, -m/--min, -a/--alphabet, and -f/--alphabet-file affect the result. \
        Non-streaming."
    )]
    consensus: Option<String>,

    #[arg(
        short('C'),
        long("mut"),
        value_name("STYLE"),
        help = "Highlight differences from the consensus with a style.",
        long_help = "Opposite of -c/--consensus. \
        Highlight mutations/deviations from consensus. \
        Does not support sequence spanning multiple lines. \
        Options -r/--regex, -m/--min, -a/--alphabet, and -f/--alphabet-file affect the result. \
        Non-streaming."
    )]
    mutations: Option<String>,

    #[arg(
        short('T'),
        long,
        help = "Transpose the sequence lines.",
        long_help = "Transpose, i.e. swap columns and rows. \
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
        hide_default_value = true,
        ignore_case = true,
        hide_possible_values = true,
        help = "Use a pager: auto (default), always, never.",
        long_help = "When to use a pager. \
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
        hide_possible_values = true,
        help = "Color depth: 16, 256, true. Overrides NO_COLOR and TERM=dumb.",
        long_help = "Color output mode. \
        \"true\": truecolor (24-bit). \
        \"256\": 256-color palette. \
        \"16\": basic 16 ANSI colors (smallest output). \
        Overrides NO_COLOR and TERM=dumb. \
        Without this flag: no styles with TERM=dumb, no colors with NO_COLOR, else the depth from COLORTERM."
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
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Convert command line arguments to a coloring configuration.
///
/// Styles: from `-k`, else none with `TERM=dumb`, else no colors with a non-empty `NO_COLOR`,
/// else the color depth from `COLORTERM`.
///
/// # Arguments
/// * `args` - Parsed command line arguments
///
/// # Returns
/// Coloring configuration.
///
/// # Errors
/// On an invalid `-c`/`-C` style, an invalid regex, an empty alphabet,
/// an invisible character without a background color,
/// or a failure to read an alphabet file or a colorscheme file.
fn config(args: &Args) -> Result<Config> {
    let builtins = colorschemes::load_colorschemes();

    let mut file_alphabet: Option<HashSet<char>> = None;
    for path in &args.alphabet_file {
        file_alphabet.get_or_insert_default().extend(bio::read_alphabet_file(path)?);
    }
    let alphabet = match (&args.alphabet, file_alphabet) {
        (None, None) => None,
        (spec, file_alphabet) => {
            Some(spec.as_ref().unwrap_or(&bio::AlphabetSpec::default()).chars(file_alphabet).context("-a/-f")?)
        }
    };

    let mut regexes = vec![];
    if args.regex != ".*" {
        regexes.push(Regex::new(&args.regex).context("-r")?);
    }
    if let Some(min_seq_length) = args.min_seq_length {
        let seq_alphabet = match &alphabet {
            Some(alphabet) => alphabet,
            None => &bio::ALPHABETS["all"].chars().collect(),
        };
        regexes.push(matches::min_length_regex(seq_alphabet, min_seq_length as usize));
    }

    let support = match args.colors.as_deref() {
        Some("16") => Support::Colors(ColorDepth::Ansi16),
        Some("256") => Support::Colors(ColorDepth::Fixed256),
        Some(_) => Support::Colors(ColorDepth::True),
        None if env::var("TERM").ok().as_deref() == Some("dumb") => Support::Plain,
        None if anstyle_query::no_color() => Support::Attributes,
        None if !anstyle_query::truecolor() => Support::Colors(ColorDepth::Fixed256),
        None => Support::Colors(ColorDepth::True),
    };

    let schemes_bg = colorschemes::read_schemes(args.background.as_deref().unwrap_or_default(), &builtins)?;
    let schemes_fg = args.foreground.as_deref().map(|names| colorschemes::read_schemes(names, &builtins)).transpose()?;
    let alphabet_seq_type = args.alphabet.as_ref().and_then(|spec| spec.keyword).and_then(bio::SeqType::from_alphabet);
    let seq_types = match alphabet_seq_type {
        Some(seq_type) => vec![seq_type],
        None => bio::SeqType::ALL.to_vec(),
    };
    let invisible = args.invisible.as_deref().map(Invisible::parse);
    if let Some(Invisible::Only(chars)) = &invisible {
        let colors_bg: Vec<_> = seq_types.iter().map(|&seq_type| colorschemes::merge_colors(&schemes_bg, seq_type)).collect();
        if let Some(c) = chars.chars().find(|c| colors_bg.iter().all(|colors| !colors.contains_key(c))) {
            bail!("-i {c:?}: character has no background color.");
        }
    }
    let styles_of = |seq_type| {
        let bg = colorschemes::merge_colors(&schemes_bg, seq_type);
        let mut fg = match &schemes_fg {
            None => default_fg(&bg),
            Some(schemes) => colorschemes::merge_colors(schemes, seq_type),
        };
        if let Some(invisible) = &invisible {
            fg.extend(bg.iter().filter(|&(&c, _)| invisible.hides(c)));
        }
        styles(&fg, &bg, support)
    };
    let styles = match alphabet_seq_type {
        Some(seq_type) => Styles::One(styles_of(seq_type)),
        None => match (styles_of(bio::SeqType::Nucl), styles_of(bio::SeqType::AA)) {
            (nucl, aa) if nucl == aa => Styles::One(nucl),
            (nucl, aa) => Styles::ByType { nucl, aa },
        },
    };

    let highlight_style = args
        .consensus
        .as_ref()
        .or(args.mutations.as_ref())
        .map(|text| colorschemes::parse_style(text).map_err(|e| anyhow!("{text}: {e}")))
        .transpose()?
        .map(|style| restrict(style, support));

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

/// Foreground colors that keep text legible on background colors.
///
/// # Arguments
/// * `bg` - Background color of each character
///
/// # Returns
/// Black on light and white on dark background colors, and gray for [`bio::GAPS`] without a background color.
fn default_fg(bg: &HashMap<char, Color>) -> HashMap<char, Color> {
    // Explicit black and white, as inverting the terminal colors breaks the pager.
    let mut fg: HashMap<char, Color> = bio::GAPS.chars().map(|gap| (gap, Color::Rgb(128, 128, 128))).collect();
    fg.extend(bg.iter().map(|(&c, &color)| (c, if is_light(color) { Black } else { White })));
    fg
}

/// Characters to hide by giving them their background color as foreground color.
enum Invisible {
    All,
    Except(String),
    Only(String),
}

impl Invisible {
    /// Parse `-i`: "." for all characters, "^" followed by the characters to not hide, or the characters to hide.
    fn parse(text: &str) -> Invisible {
        if text == "." {
            Invisible::All
        } else if let Some(visible) = text.strip_prefix('^') {
            Invisible::Except(visible.to_string())
        } else {
            Invisible::Only(text.to_string())
        }
    }

    /// Whether `c` is hidden.
    fn hides(&self, c: char) -> bool {
        match self {
            Invisible::All => true,
            Invisible::Except(visible) => !visible.contains(c),
            Invisible::Only(hidden) => hidden.contains(c),
        }
    }
}

/// Styles of characters from their colors.
///
/// # Arguments
/// * `fg` - Foreground color of each character
/// * `bg` - Background color of each character
/// * `support` - Styles the terminal can show
///
/// # Returns
/// Style of each character with a foreground or background color.
fn styles(fg: &HashMap<char, Color>, bg: &HashMap<char, Color>, support: Support) -> HashMap<char, Style> {
    combine(fg, bg).into_iter().map(|(c, style)| (c, restrict(style, support))).collect()
}

/// Write the colored files, or the colorscheme list, to stdout or a pager.
///
/// Files that do not open or read are reported on stderr, and the others are written.
///
/// # Arguments
/// * `args` - Parsed command line arguments
///
/// # Returns
/// Success, or failure if a file did not open or read.
///
/// # Errors
/// On an invalid configuration, or a write error.
fn run(args: Args) -> Result<ExitCode> {
    if args.list_colorschemes {
        let schemes = colorschemes::load_colorschemes();
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
