use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use std::collections::HashSet;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::rc::Rc;
use yansi::Color;
use yansi::Style;

use seqcol::ansi_colors::{combine, contrast_color, restrict, CharStyles, ColorDepth, Support};
use seqcol::bio::SeqType;
use seqcol::charmap::CharMap;
use seqcol::colorschemes::Scheme;
use seqcol::format::Format;
use seqcol::{bio, case, colorschemes, inout, matches, pager, Coloring, Config, Highlight, Input, Styles};

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
    long_about = "Colorize characters, with emphasis on biological sequences. \
    Commandline tool that adds ANSI codes to a text input using customizable per-character colorschemes.",
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
        help = "Alphabet: (dna|rna|nucl|aa)[x], bio. Add +chars, remove ^chars.",
        long_help = "Specify the alphabet. Affects -m/--min, -c/--consensus, -M/--mismatch, and which colors of -s/--fg and -S/--bg apply. \
        Keywords: \"dna\" (ACGTN), \"rna\" (ACGUN), \"nucl\" (ACGTUN), \"dnax\", \"rnax\", \"nuclx\" \
        (dna, rna, nucl with the IUPAC codes RYKMSWBDHV), \
        \"aa\" (20 amino acids and *), \"aax\" (aa and ambiguous BZX), \"bio\" (nuclx and aax). \
        Each keyword includes the gaps (- and .). \
        Each match is classified as amino acids, DNA, RNA, or DNA or RNA with IUPAC codes by its letters, \
        and by the k-th matches of the other lines of its file that have the same number of matches as its line, \
        to choose the colors of a colorscheme table (see -s/--fg). A keyword limits the classification to its sequence types. \
        The output of a file can then wait for up to 100 of its lines, until the classes are certain. \
        Use -a more than once for the union, e.g. -a nucl -a aa. \
        +chars adds and ^chars removes characters, e.g. -a dna^-. for no gaps, or -a aa+X. \
        Removal has priority over addition. \
        Without a keyword, +chars and ^chars apply to -f/--alphabet-file, or else to the characters of the colorschemes. \
        seqcol ignores whitespace, and the case of the keyword. Alphabet characters are case-sensitive, unless -i/--ignore-case. \
        The character after + or ^ is always literal, so ++ adds +. \
        In zsh with EXTENDED_GLOB, quote a value with ^: -a 'dna^-.'. \
        Default: the characters of the colorschemes for -m/--min, and all characters for -c/--consensus and -M/--mismatch."
    )]
    alphabet: Vec<bio::AlphabetSpec>,

    #[arg(
        short('f'),
        long,
        value_name("FILE"),
        help = "Alphabet file. You can use -f more than once.",
        long_help = "File with alphabet characters. seqcol ignores whitespace. You can use -f more than once. \
        The alphabet is the union of the files and the -a/--alphabet keywords, with the +chars and ^chars of -a/--alphabet."
    )]
    alphabet_file: Vec<String>,

    #[arg(
        short('m'),
        long("min"),
        value_name("LENGTH"),
        help = "Minimum length of a sequence to color.",
        long_help = "Minimum sequence length to color. \
        The alphabet is from -a/--alphabet and -f/--alphabet-file, or else the characters of the colorschemes (-s/--fg, -S/--bg). \
        Useful to avoid highlighting non-sequence text e.g. in a table file."
    )]
    min_seq_length: Option<u32>,

    // Options controlling how to color.
    #[arg(
        short('s'),
        long("fg"),
        value_name("COLORSCHEME"),
        help = "Text colorscheme: a builtin name (see -l), a color, or a file.",
        long_help = "Text color of each character, from a builtin colorscheme (see -l/--list-schemes), \
        a color for all characters (a name, hex #ff0000, or RGB 255,0,0), or a colorscheme file. \
        A file with the name of a builtin or a color needs a path, e.g. ./black. \
        A match of a sequence type without a table in the schemes gets only the [colors] colors. \
        You can use -s more than once. Later values take precedence, and a color replaces all earlier values. \
        Custom file format: TOML with an optional description, and tables [colors] (all text), \
        and tables named by the keywords of -a/--alphabet (e.g. [nucl], [aa]), \
        each for the sequence types of the keyword with their ambiguity codes. \
        A table overrides [colors] and the tables of more general keywords, e.g. [dna] overrides [nucl], and [nucl] overrides [nuclx]. \
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
/// Coloring configuration, and the styles and regexes for input of sequence formats and of plain text.
/// The styles differ by format only without `-s` and `-S`, and the regexes only without `-f`, `-s`, `-S`, and `-a` keywords.
///
/// # Errors
/// On an invalid `-c`/`-M` style, an invalid regex, an empty alphabet,
/// a hidden character without a background color,
/// or a failure to read an alphabet file or a colorscheme file.
fn config(args: &Args) -> Result<(Config, ByFormat<Coloring>)> {
    let builtins = colorschemes::load_colorschemes();

    let cased = |chars: &HashSet<char>| if args.ignore_case { case::both_cases(chars) } else { chars.clone() };
    let cased_alphabet = |mut alphabet: CharMap<bool>| {
        if args.ignore_case {
            case::add_other_case(&mut alphabet.by_char, &HashSet::new());
        }
        alphabet
    };

    let schemes_bg = colorschemes::read_schemes(args.background.as_deref().unwrap_or_default(), &builtins)?;
    let layers_fg = match (&args.foreground, &args.background) {
        (None, None) => {
            let builtin =
                |names: &[&str]| names.iter().map(|&name| FgLayer::Scheme(builtins[name].clone())).collect::<Vec<_>>();
            ByFormat { sequence: builtin(&["clustal", "igv"]), plain_text: builtin(&["character_class"]) }
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
            ByFormat { sequence: layers_fg.clone(), plain_text: layers_fg }
        }
    };
    // `Scheme::alphabet` has only `true` values, so laying one over another is their union.
    let scheme_alphabets = layers_fg.map(|layers_fg| {
        let schemes = FgLayer::schemes(layers_fg).chain(&schemes_bg);
        cased_alphabet(schemes.fold(CharMap::default(), |union, scheme| union.overlay(&scheme.alphabet())))
    });

    let mut file_alphabet: Option<CharMap<bool>> = None;
    for path in &args.alphabet_file {
        file_alphabet.get_or_insert_default().by_char.extend(bio::read_alphabet_file(path)?.by_char);
    }
    let specs: Vec<bio::AlphabetSpec> = args
        .alphabet
        .iter()
        .map(|spec| bio::AlphabetSpec { keyword: spec.keyword, add: cased(&spec.add), remove: cased(&spec.remove) })
        .collect();
    let keywords: Vec<&str> = specs.iter().filter_map(|spec| spec.keyword).collect();
    let alphabet_of = |scheme_alphabet: &CharMap<bool>| -> Result<CharMap<bool>> {
        let base = match &file_alphabet {
            Some(file_alphabet) => file_alphabet.clone(),
            None if !keywords.is_empty() => CharMap::default(),
            None => scheme_alphabet.clone(),
        };
        Ok(cased_alphabet(bio::alphabet(&specs, base).context("-a/-f")?))
    };
    // The -a/-f alphabet, if either is given.
    let alphabets = if specs.is_empty() && file_alphabet.is_none() {
        None
    } else {
        Some(ByFormat {
            sequence: alphabet_of(&scheme_alphabets.sequence)?,
            plain_text: alphabet_of(&scheme_alphabets.plain_text)?,
        })
    };

    let regex_any = (!args.regexps.is_empty())
        .then(|| matches::any_regex(&args.regexps, args.ignore_case))
        .transpose()
        .context("-e")?;
    let regexes_of = |min_length_alphabet: &CharMap<bool>| {
        let min_length_regex =
            args.min_seq_length.map(|min_length| matches::min_length_regex(min_length_alphabet, min_length as usize));
        Rc::new(regex_any.iter().cloned().chain(min_length_regex).collect::<Vec<_>>())
    };
    let regexes = alphabets.as_ref().unwrap_or(&scheme_alphabets).map(regexes_of);

    let support = match args.colors.as_deref() {
        Some("16") => Support::Colors(ColorDepth::Ansi16),
        Some("256") => Support::Colors(ColorDepth::Fixed256),
        Some(_) => Support::Colors(ColorDepth::True),
        None if env::var("TERM").ok().as_deref() == Some("dumb") => Support::Plain,
        None if anstyle_query::no_color() => Support::Attributes,
        None if !anstyle_query::truecolor() => Support::Colors(ColorDepth::Fixed256),
        None => Support::Colors(ColorDepth::True),
    };

    let seq_types: Vec<SeqType> = SeqType::ALL
        .into_iter()
        .filter(|seq_type| keywords.is_empty() || keywords.iter().any(|&keyword| bio::seq_types(keyword).contains(seq_type)))
        .collect();
    // A character without a color of its own in any of the layers gets the color of its other case.
    let with_other_case = |colors: &CharMap<Color>, own: &HashSet<char>| {
        let mut colors = colors.clone();
        if args.ignore_case {
            case::add_other_case(&mut colors.by_char, own);
        }
        colors
    };
    let merged_with_other_case = |colors: CharMap<Color>| {
        let own = colors.by_char.keys().copied().collect();
        with_other_case(&colors, &own)
    };
    let colors_bg = |seq_type| merged_with_other_case(colorschemes::merge_colors(&schemes_bg, seq_type));
    let mut invisible = args.hide.as_deref().map(Invisible::parse);
    if let Some(Invisible { chars, except: false }) = &invisible {
        let colors_bg: Vec<_> = seq_types.iter().map(|&seq_type| colors_bg(seq_type)).collect();
        if let Some(c) = chars.iter().filter(|&&c| colors_bg.iter().all(|bg| bg.get(c).is_none())).min() {
            bail!("-H {c:?}: character has no background color.");
        }
    }
    if let Some(invisible) = &mut invisible {
        invisible.chars = cased(&invisible.chars);
    }
    let styles_of = |layers_fg: &[FgLayer]| {
        let i_last_contrast = layers_fg.iter().rposition(|layer| matches!(layer, FgLayer::Contrast));
        let styles_of_type = |seq_type| {
            let bg = colors_bg(seq_type);
            // From the scheme colors, as the restricted colors lose lightness detail.
            let contrast = bg.map(|&color| contrast_color(color));
            // Contrast covers every character with a background color, which `bg` gives both cases.
            let own: HashSet<char> =
                FgLayer::schemes(layers_fg).flat_map(|scheme| scheme.colors(seq_type).by_char.into_keys()).collect();
            let fg = layers_fg.iter().fold(CharMap::default(), |fg, layer| {
                fg.overlay(&match layer {
                    FgLayer::Scheme(scheme) => with_other_case(&scheme.colors(seq_type), &own),
                    FgLayer::Contrast => contrast.clone(),
                })
            });
            let contrast_except: Option<HashSet<char>> = i_last_contrast.and_then(|i| {
                let schemes_after = FgLayer::schemes(&layers_fg[i + 1..]);
                let colors_after = colorschemes::merge_colors(schemes_after, seq_type);
                colors_after.other.is_none().then(|| merged_with_other_case(colors_after).by_char.into_keys().collect())
            });
            let mut by_char = combine(&fg, &bg);
            if let Some(invisible) = &invisible {
                by_char = invisible.hide(by_char);
            }
            CharStyles { by_char: by_char.map(|&style| restrict(style, support)), contrast_except }
        };
        Rc::new(styles_by_type(&seq_types, styles_of_type))
    };
    let styles = layers_fg.map(|layers_fg| styles_of(layers_fg));

    let highlight_style = args
        .consensus
        .as_ref()
        .or(args.mismatches.as_ref())
        .map(|text| colorschemes::parse_style(text).map_err(|e| anyhow!("{text}: {e}")))
        .transpose()?
        .map(|style| restrict(style, support));

    let config = Config {
        transpose: args.transpose,
        highlight: highlight_style.map(|style| Highlight {
            consensus: args.consensus.is_some(),
            style,
            alphabet: alphabets.map(|alphabets| alphabets.sequence),
            ignore_case: args.ignore_case,
        }),
        support,
    };
    let colorings = ByFormat {
        sequence: (styles.sequence, regexes.sequence),
        plain_text: (styles.plain_text, regexes.plain_text),
    };
    Ok((config, colorings))
}

/// Styles of sequence types, with the types of the same styles grouped.
///
/// # Arguments
/// * `seq_types` - Sequence types, at least one
/// * `styles_of` - Styles of a sequence type
///
/// # Returns
/// [`Styles::One`] if all types have the same styles, else [`Styles::ByType`] with one entry for each different style,
/// in the order of the first type of each entry in `seq_types`.
fn styles_by_type(seq_types: &[SeqType], styles_of: impl Fn(SeqType) -> CharStyles) -> Styles {
    let mut entries: Vec<(Vec<SeqType>, CharStyles)> = Vec::new();
    for &seq_type in seq_types {
        let styles = styles_of(seq_type);
        match entries.iter_mut().find(|(_, entry_styles)| *entry_styles == styles) {
            Some((entry_types, _)) => entry_types.push(seq_type),
            None => entries.push((vec![seq_type], styles)),
        }
    }
    match <[_; 1]>::try_from(entries) {
        Ok([(_, styles)]) => Styles::One(styles),
        Err(entries) => Styles::ByType(entries),
    }
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

    /// `f` of each value.
    fn map<U>(&self, mut f: impl FnMut(&T) -> U) -> ByFormat<U> {
        ByFormat { sequence: f(&self.sequence), plain_text: f(&self.plain_text) }
    }
}

/// Source of text colors. In a list, later layers take precedence for their characters.
#[derive(Clone)]
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

    /// Hide characters.
    ///
    /// # Arguments
    /// * `styles` - Style of each character
    ///
    /// # Returns
    /// `styles` where each hidden character with a background color has it as text color.
    fn hide(&self, mut styles: CharMap<Style>) -> CharMap<Style> {
        // `chars` get their own style, so hiding (or not hiding) them is independent of `other`.
        if let Some(other) = styles.other {
            for &c in &self.chars {
                styles.by_char.entry(c).or_insert(other);
            }
        }
        let hidden = |mut style: Style| {
            style.foreground = style.background.or(style.foreground);
            style
        };
        for (&c, style) in &mut styles.by_char {
            if self.hides(c) {
                *style = hidden(*style);
            }
        }
        if self.except {
            styles.other = styles.other.map(hidden);
        }
        styles
    }
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

    let (config, colorings) = config(&args)?;

    // Handle non-existent files like `cat`: report files that fail to open, read the rest, and exit with failure.
    // Open before the pager starts, so errors are not hidden by pager output.
    let inputs: Vec<_> = args.files.iter()
        .filter_map(|f| {
            let reader = inout::open(f).inspect_err(|e| eprintln!("{f}: {e}")).ok()?;
            let formats = args.formats.clone().unwrap_or_else(|| vec![Format::from_path(f)]);
            let (styles, regexes) = colorings.for_formats(&formats).clone();
            Some(Input { name: f.clone(), reader, formats, styles, regexes })
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
