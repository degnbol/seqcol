# Plan: Classify each run as nucleotide or amino acid by its likelihood ratio
Status: spec
Priority: P1: high (breakage)
Depends:

## Relevant files
- `src/bio.rs` — `SeqType`, `detect_seq_type`, `detect_seq_type_from_files`, `MIN_NUCLEOTIDE_FRACTION`, `N_DETECTION_LETTERS`
- `src/main.rs` — `config()`: detection, `read_schemes` for `-s`/`-S`, default fg, `-i`, color depth; `-s`/`-S`/`-a` help
- `src/lib.rs` — `run`, `Config`, `write_styled`, `paint_line`, unit tests that build `Config`
- `src/colorschemes.rs` — `get_colorscheme_names`, `load_colorschemes`, `read_colorscheme`, `resolve_scheme_name`, `read_schemes`
- `data/colorschemes/*.tsv`, `data/README.md`
- `tests/integration.rs`, `tests/data/`
- `BUGS.md`, `AGENTS.md`
- Skills: `/implement`, `/test-driven-development`

## Background
A base scheme name (e.g. `-s taylor`) selects `_nucl` or `_aa` from the detected sequence type.
Detection now gives one type for all input. It reads files before the main read:
- A table with a DNA column and a protein column gets one scheme for both.
- Stdin is not read, so a base scheme name fails. A FIFO is read twice, so its input is lost.
- FASTQ is detected as AA: quality lines (e.g. `FFFF:FFF`) are in the sample.
- IUPAC-dense primers (`GAYCARATHGAYGG`, 71% ACGTUN) are detected as AA by the 90% rule.

The new design classifies each run (each range matched by the regexes) while it streams.

Score of a run: a likelihood ratio test.
Each letter is independent, from a nucleotide distribution or from an amino acid distribution.
The log odds of a text is the sum of ln(P_nucl(c) / P_aa(c)) over its letters c.
A run is decided when |log odds| ≥ ln(p/(1−p)) = ln 99 ≈ 4.6, where p = 0.99 is the posterior threshold.
- ACGT letters add +0.9 (A) to +2.7 (C) each, so 3 to 6 decide Nucl.
- One protein-only letter (E, F, I, L, P, Q) adds −6 to −7. It decides AA in a short run. About 5 ACGT letters outweigh it, and about 8 decide Nucl.
- IUPAC codes have a nucleotide probability, so a few of them in ACGT text do not decide AA.

Short runs: an undecided run (e.g. `-----------A` in a wrapped protein) uses the evidence of the other runs at the same position.
Mixed types at the same position are rare in FASTA and FASTQ.
To give the first lines that evidence, the first lines are read ahead before output.
The read-ahead buffers lines in memory, so it works on stdin and FIFOs.

Nothing reads DNA vs RNA, so `SeqType` keeps only nucleotide vs amino acid.

## Changes

### Letter frequencies
`data/letter_frequencies.tsv` (new): columns `letter`, `nucl`, `aa`, with a `#` comment that cites the sources.
- `aa`: measured frequencies of the 20 amino acids (UniProtKB/Swiss-Prot release statistics), copied, not regenerated.
- `nucl`: model values, not measured, that sum to 1: A, C, G, T 0.2 each; N 0.1; IUPAC R Y K M S W B D H V 0.01 each.
  U has the probability of T: the model is DNA or RNA, and U replaces T in RNA.
  N (0.1) is more probable than Asn (≈0.04), so N-only runs (scaffold gaps, failed reads) are Nucl.
- B (IUPAC C/G/T, and Asx) gets 0.01 in both columns, so it is neutral. Else one B (ln 100 > ln 99) decides Nucl.
- A letter not in a column gets the floor `1e-4`, so no letter has infinite log odds.
- `data/README.md`: cite the file.

### `src/bio.rs`
- `SeqType` becomes `{ Nucl, AA }`. `from_alphabet` maps `dna`, `rna`, `nucl` and their `nogap` variants to `Nucl`.
- `pub const MIN_ABS_LOG_ODDS: f64` = ln 99.
- `pub fn nucl_log_odds(text: &[u8]) -> f64`: ln(P(text | nucleotide) / P(text | amino acid)). Case-insensitive. Non-letters add 0.
  It sums a `static LazyLock<[f64; 256]>` table of log odds per byte, parsed once from `include_str!("../data/letter_frequencies.tsv")`.
- `impl SeqType { pub fn from_log_odds(run: f64, others: f64) -> SeqType }`:
  the sign of `run` if |`run`| ≥ `MIN_ABS_LOG_ODDS`, else the sign of `run + others`. 0 gives `AA`.
  `others` is the evidence of the other runs at the same position, without `run`.
- Remove `detect_seq_type`, `detect_seq_type_from_files`, `MIN_NUCLEOTIDE_FRACTION`, `N_DETECTION_LETTERS`, their tests, and the `inout` and `intersect_matches` imports.

### `src/lib.rs`
- `pub enum Styles { One(HashMap<char, Style>), ByType { nucl: HashMap<char, Style>, aa: HashMap<char, Style> } }`. `Config.styles` has this type.
  With `One`, no run is classified, and output streams as now.
- `struct Evidence(HashMap<(usize, usize), f64>)`. The key of a run is (number of runs on its line, index of the run on its line),
  so a row with a missing run (e.g. a cell under `-m`) does not take evidence from other columns.
  - `fn add(&mut self, runs_log_odds: &[f64])`: adds the log odds of the runs of one line.
  - `fn seq_types(&self, runs_log_odds: &[f64]) -> Vec<SeqType>`: `SeqType::from_log_odds(run, total − run)` for each run of a line that was added.
  Each line is added once, before its classification, so the read-ahead and streaming use the same calls.
- Evidence is for each input file: `write_styled(lines: impl Iterator<Item = (usize, Vec<u8>)>, output, config)` gets each line with the index of its reader from `run`.
  A new reader index starts a new `Evidence` and, when streaming, a new read-ahead. So `seqcol dna.fa prot.fa` does not mix types.
  The first line of a new reader ends the current read-ahead, and it is the first line of the next read-ahead (use a `Peekable` iterator).
  Consensus (`-c`, `-C`) and transpose still use the lines of all files together, as now.
- `write_styled` with `Styles::ByType`. Each line keeps its ranges (`intersect_matches`, computed once) and the log odds of its runs.
  - Streaming: read ahead and add lines until one of these is true:
    at least one position was seen and every seen position has |total| ≥ `MIN_ABS_LOG_ODDS`; `MAX_READ_AHEAD_LINES` (100) lines; end of the reader.
    Then classify and paint the buffered lines. Then, for each later line: add, classify, paint.
  - Not streaming (`-T`, `-c`, `-C`): add all lines of a reader, then classify and paint each of its lines.
- `fn paint_line(line: &[u8], runs: &[(Range<usize>, &HashMap<char, Style>)]) -> Vec<Char>`: paints each range with its styles.
- Unit tests: build `Config` with `Styles::One`. New tests: `Evidence` gives an undecided run the type of the other runs; it does not count a run twice.

### `src/main.rs`
- `config()` no longer calls detection. It reads each `-s`/`-S` scheme once, then merges the colors for each type.
- `Styles::One` if `-a` gives a type (its styles), or if the final styles (after `default_fg`, `hide`, `styles`) are equal for both types. Else `Styles::ByType`.
  With `-a`, `-i` characters are checked only against the bg colors of that type.
- New `fn default_fg(bg: &HashMap<char, Color>) -> HashMap<char, Color>`: the current inline block.
- New `fn hide(fg: &mut HashMap<char, Color>, bg: &HashMap<char, Color>, invisible: &str)`: `-i` for one type, without a panic.
  `config()` panics once if an `-i` character has a bg color for neither type. A scheme for one type leaves the other type's colors empty, e.g. `-s jalview -i A`.
- New `fn styles(fg, bg, depth) -> HashMap<char, Style>`: `combine` and `reduce_depth`. The depth is computed once.
- Help of `-s`, `-S`, `-a`: describe classification per run, `#type:` sections, that runs of a type without a scheme are not colored,
  and that the first output waits for up to 100 lines unless `-a` gives a type. Short help: `plans/short-help.md`.

### `src/colorschemes.rs` and scheme files
Type sections: the comment line `#type: nucl` or `#type: aa` (exact, lowercase, no space after `#`) starts a section of that type.
Lines before the first `#type:` fit both types. Builtin and custom files use the same syntax.
A scheme name is its file name. Suffixes, base names and their resolution are removed.
- Builtins: merge each `<name>_nucl.tsv` and `<name>_aa.tsv` into `<name>.tsv` with two sections (`chemistry`, `shapely`, `taylor`, `zappo`).
  A merged file has one description at the top that covers both types, written from the two old descriptions.
  Rename the others to `<name>.tsv` with one section (e.g. `clustal`, `jalview`, `jalview_ambiguity`).
- `-s jalview -s clustal` colors nucleotide runs with jalview and amino acid runs with clustal.
  Two schemes with both sections cannot be mixed by type (e.g. taylor for nucleotides with zappo for amino acids): see `IDEAS.md`.
- New `pub struct Scheme { pub nucl: HashMap<char, Color>, pub aa: HashMap<char, Color>, pub description: String }`.
  Lines that fit both types go into both maps.
- New `fn parse_colorscheme(text: &str) -> Result<Scheme>`: one parser for builtin and custom files.
  `load_colorschemes`, `read_colorscheme` and `get_colorscheme_names` use it. The description is the `# ` lines before the first color line.
- `pub fn read_schemes(names: &[String], builtins: &HashMap<String, Scheme>) -> Result<Vec<Scheme>>`: a builtin name, else a file, read once.
- `pub fn merge_colors(schemes: &[Scheme], seq_type: SeqType) -> HashMap<char, Color>`: the map of `seq_type` of each scheme. Later schemes take precedence.
- Remove `resolve_scheme_name` and `SeqType::suffix`.
- `letter_aa.tsv` colors all 26 letters: rename it to `letter.tsv`, without a section, and edit its description to not say amino acids.
- `-l` shows the types of each scheme, e.g. `taylor (nucl, aa)`, as the name no longer shows them. `write_colorscheme_list` takes the types.
- Unit test: each builtin parses. The merged builtins and `letter` have colors for both types; every other builtin has colors for exactly one type.

### Tests
- Unit, `nucl_log_odds`:
  - `ACGTNNACGT`, `ACGUACGU`, `NNNNNNNNNN`, and the primer `GAYCARATHGAYGG` give > 0.
  - `MTSNRARVTYNPPPTTTGTR` and `GATAMKLV` (a protein that starts with ACGT letters) give < 0.
  - `""` and `--- 12\t` give 0.
- Unit, `SeqType::from_log_odds`: a decided run ignores `others`; an undecided run uses them; 0 gives `AA`.
- Unit, `parse_colorscheme`: reads `#type:`, and leaves it out of the description.
- Integration, `tests/data/mixed.tsv` (new) with a DNA column and a protein column:
  - `-s taylor -m 8` colors the DNA cells with the taylor nucleotide colors and the protein cells with the taylor amino acid colors.
  - `-s jalview -m 8` colors the DNA cells, and the protein cells are not colored.
- Integration: `-s taylor` on stdin and on a FIFO colors DNA with the taylor nucleotide colors.
- Integration: a custom scheme given as a FIFO is read.
- Integration: `-s taylor dna.fa prot.fa`, where `prot.fa` starts with `-----------A`, colors each file with its own type.
- Integration, `tests/data/reads.fq` (new): sequence lines get the taylor nucleotide colors.
- Integration, `tests/data/wrapped_aa.fa`: the first sequence line `-----------A` gets the taylor amino acid colors (the header must not end the read-ahead).
- Changed: suffixed builtin names in `tests/integration.rs`, `tests/expected.sh`, `tests/pager_display.sh` and `BUGS.md` (e.g. `taylor_nucl` → `taylor`, `hydrophobicity_aa` → `hydrophobicity`).
  Use `-a` only where the test forces a type (`test_alphabet_flag_overrides_auto_detect`, the `test_auto_detect_*_with_base_name` comparisons), as `-a` also changes `-m` and the consensus.
  `test_output_size` values stay: its fixtures are protein.
- Changed: `test_auto_detect_error_no_matching_suffix` (`-s hydrophobicity` on DNA): the DNA is not colored, with no error. Rename it.
- Changed: `test_alphabet_flag_overrides_auto_detect`: compare `-s taylor -a aa` with an output that `-a aa` also forces.

### Docs
- `BUGS.md`: remove the stdin and process substitution entries.
- `AGENTS.md`: the `bio.rs` line; Performance Notes: the read-ahead.

## Expected outcome
- Each run is colored with the scheme of its own sequence type. Mixed tables color each column correctly.
- A scheme for one type colors only runs of that type.
- Schemes with both types work on stdin and FIFOs. No input or scheme file is read twice.
- FASTQ sequence lines, N-only runs and degenerate primers are nucleotides.
- With `-a` or schemes that fit both types, output streams line by line as now.

## Non-goals
- Quality lines are still colored ("FASTQ quality lines are colored" in `BUGS.md`), now with the amino acid scheme.
- Lowercase letters in colorschemes and alphabets.
- `.` as a gap in `-m` runs.
- Mixed types at the same position, e.g. a FASTA file with DNA and protein records.
- Motif regexes: with `-r A`, each run is one letter, so it is classified Nucl. The evidence map also grows with each distinct number of runs per line.
- Short `-h` help: `plans/short-help.md`. A man page: `IDEAS.md`.
