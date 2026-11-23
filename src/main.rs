use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use yansi::Color::{self, *};
use yansi::Style;

use seqcol::ansi_colors::{combine, is_light, restrict, ColorDepth, Support};
use seqcol::format::Format;
use seqcol::{bio, case, colorschemes, inout, matches, pager, Config, Highlight, Styles};

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
    about = "Colorize strings based on their characters.",
    long_about = "Colorize biological sequences (amino acids, DNA, and RNA). \
    Useful for viewing fasta files, sequence alignments, CSV, TSV, and other text files. \
    A simple commandline tool like `cat`, which may be useful for colorizing \
    strings in general.",
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
        long_help = "Text containing sequences, plain or gzip-compressed. Default is reading stdin. \
        ANSI escape codes in the input are not text to match or transpose. \
        Text outside the matches keeps the style of its ANSI style codes."
    )]
    files: Vec<String>,

    // Options controlling what to color.
    #[arg(
        short('e'),
        long("regexp"),
        value_name("PATTERN"),
        help = "Color only text that matches a regex. You can use -e more than once.",
        long_help = "Color only text that matches one of the given regex patterns. You can use -e more than once. \
        A match is text that a pattern matches, and -m/--min too if given. \
        Useful for only highlighting restriction sites, binding sites etc. \
        Patterns match UTF-8 text, so . does not match an invalid byte. \
        Prefix (?-u) to match bytes. A flag such as (?-u) applies only to its pattern. \
        Where matches of patterns overlap, the first pattern wins, as in rg. \
        -i/--ignore-case makes all patterns case-insensitive. (?-i) in a pattern overrides it. \
        Default: all text."
    )]
    regexps: Vec<String>,

    #[arg(
        short('i'),
        long,
        help = "Ignore case in -e, the alphabet, the consensus, and the colors.",
        long_help = "Ignore case in -e/--regexp, -a/--alphabet, -f/--alphabet-file, -m/--min, -c/--consensus, -M/--mismatch, \
        and -H/--hide. \
        A character without a color of its own gets the color of its other case, \
        e.g. a gets the color of A unless a colorscheme has a color for a."
    )]
    ignore_case: bool,

    #[arg(
        short('t'),
        long("type"),
        value_name("FORMATS"),
        value_enum,
        value_delimiter(','),
        default_value = "fasta,fastq",
        hide_default_value = true,
        hide_possible_values = true,
        help = "Input formats: fasta, fastq, text. Default: fasta,fastq.",
        long_help = "Formats of the input, separated by commas. \
        Lines that are not sequence in one of the formats are not colored or transposed. \
        \"fasta\": lines that start with > (header lines) are not sequence. \
        \"fastq\": lines that start with @ or + (header and separator lines), and quality lines of records, \
        are not sequence. \
        \"text\": all lines can be sequence. \
        Default: fasta,fastq."
    )]
    formats: Vec<Format>,

    #[arg(
        short('a'),
        long,
        help = "Alphabet: dna, rna, nucl, aa, aax, all. Add +chars, remove ^chars.",
        long_help = "Specify the alphabet. Affects -m/--min, -c/--consensus, -M/--mismatch, and which colors of -s/--bg and -S/--fg apply. \
        \"dna\", \"rna\", and \"nucl\" give every match the nucleotide colors. \
        \"aa\" and \"aax\" give every match the amino acid colors. \
        Without these keywords, each match is classified as nucleotides or amino acids by its letters, \
        and by the k-th matches of the other lines of its file that have the same number of matches as its line. \
        The output of a file can then wait for up to 100 of its lines, until the classes are certain. \
        Keywords: \"dna\" (ACGTN), \"rna\" (ACGUN), \"nucl\" (ACGTUN), \
        \"aa\" (20 amino acids and *), \"aax\" (aa and ambiguous BZX), \"all\" (A-Z and *). Each keyword includes the gaps (- and .). \
        +chars adds and ^chars removes characters, e.g. -a dna^-. for no gaps, or -a aa+X. \
        Removal has priority over addition. \
        seqcol ignores whitespace, and the case of the keyword. Alphabet characters are case-sensitive, unless -i/--ignore-case. \
        The character after + or ^ is always literal, so ++ adds +. \
        Without a keyword, +chars and ^chars apply to -f/--alphabet-file, or else to \"all\". \
        In zsh with EXTENDED_GLOB, quote a value with ^: -a 'dna^-.'. \
        Default: \"all\" for -m/--min, and all characters for -c/--consensus and -M/--mismatch."
    )]
    alphabet: Option<bio::AlphabetSpec>,

    #[arg(
        short('f'),
        long,
        value_name("FILE"),
        help = "Alphabet file. You can use -f more than once.",
        long_help = "File with alphabet characters. seqcol ignores whitespace. You can use -f more than once. \
        The alphabet is the union of the files and the -a/--alphabet keyword, with the +chars and ^chars of -a/--alphabet."
    )]
    alphabet_file: Vec<String>,

    #[arg(
        short('m'),
        long("min"),
        value_name("LENGTH"),
        help = "Minimum length of a sequence to color.",
        long_help = "Minimum sequence length to color. \
        The alphabet is from -a/--alphabet and -f/--alphabet-file, or \"all\" without them. \
        Useful to avoid highlighting non-sequence text e.g. in a table file."
    )]
    min_seq_length: Option<u32>,

    // Options controlling how to color.
    #[arg(
        short('s'),
        long("bg"),
        value_name("COLORSCHEME"),
        help = "Background colorscheme: a builtin name (see -l) or a file.",
        long_help = "Name of a builtin colorscheme or a file with a custom colorscheme, to control the background color of each given character. \
        A scheme has colors for nucleotides, amino acids, or both (see -a/--alphabet). \
        A match of nucleotides is not colored if no scheme has nucleotide colors, and the same for amino acids. \
        You can use -s more than once; later schemes take precedence. \
        Use -l/--list-schemes for available colorschemes. \
        Custom file format: each line has a character and color separated by tab/comma/etc. \
        Colors can be names, hex (#ff0000), or RGB (255 0 0). \
        The line \"#type: nucl\" or \"#type: aa\" starts the nucleotide or amino acid colors; colors before it are for both."
    )]
    background: Option<Vec<String>>,

    #[arg(
        short('S'),
        long("fg"),
        value_name("COLORSCHEME"),
        help = "Foreground colorscheme, like -s.",
        long_help = "Same as -s/--bg but controls foreground (text) color instead of background. \
        Default: white/black text based on background lightness, gray for gaps. \
        Modified by -H/--hide."
    )]
    foreground: Option<Vec<String>>,

    #[arg(
        short('H'),
        long,
        value_name("CHARS"),
        help = "Characters to hide: text color is the background color.",
        long_help = "Hide characters by showing the text color the same as the background color (-s/--bg). \
        The argument is the characters to hide. \
        With a ^ prefix, hide every character with a background color except the characters after ^, so -H ^ hides all of them. \
        In zsh with EXTENDED_GLOB, quote a value with ^: -H '^'. \
        -i/--ignore-case also ignores case in these characters. \
        Takes precedence over -S/--fg."
    )]
    hide: Option<String>,

    // Operations.

    #[arg(
        short('c'),
        long("consensus"),
        value_name("STYLE"),
        help = "Highlight the consensus with a style, e.g. \"bold\".",
        long_help = "Compute the consensus at each character position, and show it on each line by a style: \
        \"bold\", \"underline\", \"italic\", a background color, or a combination (e.g. \"bold red\"). \
        Ties: no character at that position is highlighted. \
        Does not support records spanning multiple lines. \
        Options -e/--regexp, -t/--type, -m/--min, -a/--alphabet, -f/--alphabet-file, and -i/--ignore-case affect the result. \
        Non-streaming."
    )]
    consensus: Option<String>,

    #[arg(
        short('M'),
        long("mismatch"),
        value_name("STYLE"),
        help = "Highlight differences from the consensus with a style.",
        long_help = "Highlight the characters that differ from the consensus at their position, with a style like -c/--consensus. \
        Ties: no character at that position is highlighted. \
        Ignored with -c/--consensus. \
        Does not support records spanning multiple lines. \
        Options -e/--regexp, -t/--type, -m/--min, -a/--alphabet, -f/--alphabet-file, and -i/--ignore-case affect the result. \
        Non-streaming."
    )]
    mismatches: Option<String>,

    #[arg(
        short('T'),
        long,
        help = "Transpose the lines with a match.",
        long_help = "Transpose, i.e. swap columns and rows. \
        Only lines with a match are transposed (see -e/--regexp, -t/--type, and -m/--min). \
        Other lines (e.g. fasta headers) are written first, unchanged. \
        Does not support records spanning multiple lines. \
        May be useful for scrolling long lines. \
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
        Default pager is `less -SK~#8RXL --mouse --wheel-lines=5`. $LESS options override these options. $PAGER replaces less."
    )]
    paging: PagingMode,

    #[arg(
        short('k'),
        long("colors"),
        alias("colours"),
        value_name("MODE"),
        value_parser(["16", "256", "true"]),
        hide_possible_values = true,
        help = "Color depth: 16, 256, true. Overrides NO_COLOR and TERM=dumb.",
        long_help = "Color depth. \
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
/// On an invalid `-c`/`-M` style, an invalid regex, an empty alphabet,
/// a hidden character without a background color,
/// or a failure to read an alphabet file or a colorscheme file.
fn config(args: &Args) -> Result<Config> {
    let builtins = colorschemes::load_colorschemes();

    let mut file_alphabet: Option<HashSet<char>> = None;
    for path in &args.alphabet_file {
        file_alphabet.get_or_insert_default().extend(bio::read_alphabet_file(path)?);
    }
    let cased = |chars: HashSet<char>| if args.ignore_case { case::both_cases(&chars) } else { chars };
    let alphabet = match (&args.alphabet, file_alphabet) {
        (None, None) => None,
        (spec, file_alphabet) => Some(cased(
            spec.as_ref().unwrap_or(&bio::AlphabetSpec::default()).chars(file_alphabet).context("-a/-f")?,
        )),
    };

    let mut regexes = vec![];
    if !args.regexps.is_empty() {
        regexes.push(matches::any_regex(&args.regexps, args.ignore_case).context("-e")?);
    }
    if let Some(min_seq_length) = args.min_seq_length {
        let seq_alphabet = match &alphabet {
            Some(alphabet) => alphabet,
            None => &cased(bio::ALPHABETS["all"].chars().collect()),
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
    let merged_colors = |schemes, seq_type| {
        let mut colors = colorschemes::merge_colors(schemes, seq_type);
        if args.ignore_case {
            case::add_other_case(&mut colors);
        }
        colors
    };
    let mut invisible = args.hide.as_deref().map(Invisible::parse);
    if let Some(Invisible { chars, except: false }) = &invisible {
        let colors_bg: Vec<_> = seq_types.iter().map(|&seq_type| merged_colors(&schemes_bg, seq_type)).collect();
        if let Some(c) = chars.iter().filter(|c| colors_bg.iter().all(|bg| !bg.contains_key(c))).min() {
            bail!("-H {c:?}: character has no background color.");
        }
    }
    if let Some(invisible) = &mut invisible {
        invisible.chars = cased(std::mem::take(&mut invisible.chars));
    }
    let styles_of = |seq_type| {
        let bg = merged_colors(&schemes_bg, seq_type);
        let mut fg = match &schemes_fg {
            None => default_fg(&bg),
            Some(schemes) => merged_colors(schemes, seq_type),
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
        formats: args.formats.clone(),
        transpose: args.transpose,
        highlight: highlight_style.map(|style| Highlight {
            consensus: args.consensus.is_some(),
            style,
            alphabet,
            ignore_case: args.ignore_case,
        }),
        support,
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
struct Invisible {
    chars: HashSet<char>,
    /// Hide every character except `chars`, else only `chars`.
    except: bool,
}

impl Invisible {
    /// Parse the characters to hide.
    ///
    /// # Arguments
    /// * `text` - "^" followed by the characters to not hide, or the characters to hide
    fn parse(text: &str) -> Invisible {
        match text.strip_prefix('^') {
            Some(visible) => Invisible { chars: visible.chars().collect(), except: true },
            None => Invisible { chars: text.chars().collect(), except: false },
        }
    }

    /// Whether `c` is hidden.
    fn hides(&self, c: char) -> bool {
        self.chars.contains(&c) != self.except
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
