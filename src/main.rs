use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::rc::Rc;
use yansi::Color;
use yansi::Style;

use seqcol::ansi_colors::{combine, contrast_color, restrict, CharStyles, ColorDepth, Support};
use seqcol::colorschemes::Scheme;
use seqcol::format::Format;
use seqcol::{bio, case, colorschemes, inout, matches, pager, Config, Highlight, Input, Styles};

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
    about = "Colorize characters.",
    long_about = "Sequence colorizer, with emphasis on biological sequences.",
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
        hide_possible_values = true,
        help = "Input formats: fasta, fastq, text. Default: from the file extension.",
        long_help = "Formats of the input, separated by commas. \
        Lines that are not sequence in one of the formats are not colored or transposed. \
        \"fasta\": lines that start with > (header lines) are not sequence. \
        \"fastq\": lines that start with @ or + (header and separator lines), and quality lines of records, \
        are not sequence. \
        \"text\": all lines can be sequence. \
        Default: from the file extension, also with .gz, e.g. fasta for .fa and .fasta, fastq for .fq and .fastq. \
        text for other files, stdin, and process substitution."
    )]
    formats: Option<Vec<Format>>,

    #[arg(
        short('a'),
        long,
        help = "Alphabet: dna, rna, nucl, aa, aax, all. Add +chars, remove ^chars.",
        long_help = "Specify the alphabet. Affects -m/--min, -c/--consensus, -M/--mismatch, and which colors of -s/--fg and -S/--bg apply. \
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
        long("fg"),
        value_name("COLORSCHEME"),
        help = "Text colorscheme: a builtin name (see -l) or a file.",
        long_help = "Name of a builtin colorscheme or a file with a custom colorscheme, to control the text color of each given character. \
        A scheme has colors for nucleotides, amino acids, or both (see -a/--alphabet). \
        A match of nucleotides is not colored if no scheme has nucleotide colors, and the same for amino acids. \
        You can use -s more than once; later values take precedence. \
        Use -l/--list-schemes for available colorschemes. \
        Custom file format: TOML with a description string and the tables [colors] (both sequence types), [nucl] and [aa] (override [colors]), all optional. \
        Each table has one-character keys and color values: names, hex (\"#ff0000\"), or RGB (\"255 0 0\"). \
        The value contrast gives black text on light and white text on dark background colors, \
        from -S/--bg or else from the input, e.g. -s contrast -S taylor. \
        Default without -S/--bg: character_class for input of format text, \
        and igv for nucleotides and clustal for amino acids in fasta and fastq. \
        Default with -S/--bg: no text color. \
        Modified by -H/--hide."
    )]
    foreground: Option<Vec<String>>,

    #[arg(
        short('S'),
        long("bg"),
        value_name("COLORSCHEME"),
        help = "Background colorscheme, like -s.",
        long_help = "Same as -s/--fg but controls the background color."
    )]
    background: Option<Vec<String>>,

    #[arg(
        short('H'),
        long,
        value_name("CHARS"),
        help = "Characters to hide: text color is the background color.",
        long_help = "Hide characters by showing the text color the same as the background color (-S/--bg). \
        The argument is the characters to hide. \
        With a ^ prefix, hide every character with a background color except the characters after ^, so -H ^ hides all of them. \
        In zsh with EXTENDED_GLOB, quote a value with ^: -H '^'. \
        -i/--ignore-case also ignores case in these characters. \
        Takes precedence over -s/--fg."
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
/// Coloring configuration, and the styles for input of sequence formats and of plain text.
/// The styles differ by format only without `-s` and `-S`.
///
/// # Errors
/// On an invalid `-c`/`-M` style, an invalid regex, an empty alphabet,
/// a hidden character without a background color,
/// or a failure to read an alphabet file or a colorscheme file.
fn config(args: &Args) -> Result<(Config, ByFormat<Rc<Styles>>)> {
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
    let alphabet_seq_type = args.alphabet.as_ref().and_then(|spec| spec.keyword).and_then(bio::SeqType::from_alphabet);
    let seq_types = match alphabet_seq_type {
        Some(seq_type) => vec![seq_type],
        None => bio::SeqType::ALL.to_vec(),
    };
    let with_other_case = |mut colors: HashMap<char, Color>| {
        if args.ignore_case {
            case::add_other_case(&mut colors);
        }
        colors
    };
    let colors_bg = |seq_type| with_other_case(colorschemes::merge_colors(&schemes_bg, seq_type));
    let mut invisible = args.hide.as_deref().map(Invisible::parse);
    if let Some(Invisible { chars, except: false }) = &invisible {
        let colors_bg: Vec<_> = seq_types.iter().map(|&seq_type| colors_bg(seq_type)).collect();
        if let Some(c) = chars.iter().filter(|c| colors_bg.iter().all(|bg| !bg.contains_key(c))).min() {
            bail!("-H {c:?}: character has no background color.");
        }
    }
    if let Some(invisible) = &mut invisible {
        invisible.chars = cased(std::mem::take(&mut invisible.chars));
    }
    let styles_of = |layers_fg: &[FgLayer]| {
        let i_last_contrast = layers_fg.iter().rposition(|layer| matches!(layer, FgLayer::Contrast));
        let styles_of_type = |seq_type| {
            let bg = colors_bg(seq_type);
            let mut fg = with_other_case(colorschemes::merge_colors(FgLayer::schemes(layers_fg), seq_type));
            let hidden: HashMap<char, Color> = match &invisible {
                Some(invisible) => bg.iter().filter(|&(&c, _)| invisible.hides(c)).map(|(&c, &color)| (c, color)).collect(),
                None => HashMap::new(),
            };
            let contrast_except: Option<HashSet<char>> = i_last_contrast.map(|i| {
                let schemes_after = FgLayer::schemes(&layers_fg[i + 1..]);
                let colors_after = with_other_case(colorschemes::merge_colors(schemes_after, seq_type));
                colors_after.into_keys().chain(hidden.keys().copied()).collect()
            });
            // From the scheme colors, as the restricted colors lose lightness detail.
            if let Some(except) = &contrast_except {
                fg.extend(bg.iter().filter(|(c, _)| !except.contains(c)).map(|(&c, &color)| (c, contrast_color(color))));
            }
            fg.extend(hidden);
            CharStyles { by_char: styles(&fg, &bg, support), contrast_except }
        };
        Rc::new(match alphabet_seq_type {
            Some(seq_type) => Styles::One(styles_of_type(seq_type)),
            None => match (styles_of_type(bio::SeqType::Nucl), styles_of_type(bio::SeqType::AA)) {
                (nucl, aa) if nucl == aa => Styles::One(nucl),
                (nucl, aa) => Styles::ByType { nucl, aa },
            },
        })
    };
    let styles = match (&args.foreground, &args.background) {
        (None, None) => {
            let builtin =
                |names: &[&str]| names.iter().map(|&name| FgLayer::Scheme(builtins[name].clone())).collect::<Vec<_>>();
            ByFormat {
                sequence: styles_of(&builtin(&["clustal", "igv"])),
                plain_text: styles_of(&builtin(&["character_class"])),
            }
        }
        (names_fg, _) => {
            let layers_fg = names_fg
                .iter()
                .flatten()
                .filter(|name| !name.is_empty())
                .map(|name| match name.as_str() {
                    "contrast" => Ok(FgLayer::Contrast),
                    _ => colorschemes::read_scheme(name, &builtins).map(FgLayer::Scheme),
                })
                .collect::<Result<Vec<_>>>()?;
            let styles = styles_of(&layers_fg);
            ByFormat { sequence: styles.clone(), plain_text: styles }
        }
    };

    let highlight_style = args
        .consensus
        .as_ref()
        .or(args.mismatches.as_ref())
        .map(|text| colorschemes::parse_style(text).map_err(|e| anyhow!("{text}: {e}")))
        .transpose()?
        .map(|style| restrict(style, support));

    let config = Config {
        regexes,
        transpose: args.transpose,
        highlight: highlight_style.map(|style| Highlight {
            consensus: args.consensus.is_some(),
            style,
            alphabet,
            ignore_case: args.ignore_case,
        }),
        support,
    };
    Ok((config, styles))
}

/// A value for each kind of input format.
struct ByFormat<T> {
    /// Value for input with a sequence format.
    sequence: T,
    /// Value for input of format [`Format::Text`].
    plain_text: T,
}

impl<T> ByFormat<T> {
    /// The value for input in `formats`: `plain_text` if each format is [`Format::Text`], else `sequence`.
    fn for_formats(&self, formats: &[Format]) -> &T {
        if formats.iter().all(|&format| format == Format::Text) { &self.plain_text } else { &self.sequence }
    }
}

/// Source of text colors. In a list, later layers take precedence for their characters.
enum FgLayer {
    /// Text colors of a colorscheme.
    Scheme(Scheme),
    /// Text colors by background lightness, see [`seqcol::ansi_colors::contrast`].
    Contrast,
}

impl FgLayer {
    /// The schemes of `layers`, in order.
    fn schemes(layers: &[FgLayer]) -> impl Iterator<Item = &Scheme> {
        layers.iter().filter_map(|layer| match layer {
            FgLayer::Scheme(scheme) => Some(scheme),
            FgLayer::Contrast => None,
        })
    }
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

    let (config, styles) = config(&args)?;

    // Handle non-existent files like `cat`: report files that fail to open, read the rest, and exit with failure.
    // Open before the pager starts, so errors are not hidden by pager output.
    let inputs: Vec<_> = args.files.iter()
        .filter_map(|f| {
            let reader = inout::open(f).inspect_err(|e| eprintln!("{f}: {e}")).ok()?;
            let formats = args.formats.clone().unwrap_or_else(|| vec![Format::from_path(f)]);
            let styles = styles.for_formats(&formats).clone();
            Some(Input { name: f.clone(), reader, formats, styles })
        })
        .collect();
    let all_opened = inputs.len() == args.files.len();

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

    let result = seqcol::run(inputs, output, &config);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_formats_gives_plain_text_only_for_text() {
        let by_format = ByFormat { sequence: "sequence", plain_text: "plain_text" };
        assert_eq!(*by_format.for_formats(&[Format::Text]), "plain_text");
        assert_eq!(*by_format.for_formats(&[]), "plain_text");
        assert_eq!(*by_format.for_formats(&[Format::Fasta]), "sequence");
        assert_eq!(*by_format.for_formats(&[Format::Fasta, Format::Text]), "sequence");
    }
}
