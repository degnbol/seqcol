# Plan: Short help with `-h`, details with `--help`
Status: spec
Priority: P2: medium (cosmetic)
Depends: detect-seq-type-sprt.md

## Relevant files
- `src/main.rs` — `Args`: `#[command(...)]` and each `#[arg(help = ...)]`
- `tests/integration.rs`
- `README.md:21` — shows `seqcol --help`
- Skills: `/implement`, `/writing`

## Background
Each argument has one `help` text of up to 8 sentences, so `-h` and `--help` both print all of it.
clap prints an argument's `help` with `-h`, and its `long_help` with `--help`.
When any argument has a `long_help`, clap changes the `-h, --help` entry to "Print help (see more with '--help')" in `-h`,
and "Print help (see a summary with '-h')" in `--help`. So the difference is shown with no extra text.
Many CLI tools work this way (ripgrep, fd, bat, hyperfine).

## Changes
`src/main.rs` `Args`:
- Each argument with more than one sentence: `help` becomes one short line, and the current text moves to `long_help`.
  Proposed `help` lines (the `long_help` text is the help after the depended plan):
  - `FILE`: "Input files, plain or gzip-compressed. Default: stdin."
  - `-s`: "Background colorscheme: a builtin name (see -l) or a file."
  - `-S`: "Foreground colorscheme, like -s."
  - `-a`: "Alphabet: dna, rna, nucl, aa, aax, all, or a file."
  - `-i`: "Characters to hide: text color is the background color."
  - `-m`: "Minimum length of a sequence run to color."
  - `-r`: "Color only text that matches this regex."
  - `-c`: "Highlight the consensus with a style, e.g. \"bold\"."
  - `-C`: "Highlight differences from the consensus with a style."
  - `-T`: "Transpose the sequence lines."
  - `-p`: "When to use a pager: auto, always, never."
  - `-k`: "Color depth: 16, 256, true. Default: from COLORTERM/TERM."
  - `-l`: unchanged ("List available colorschemes."), no `long_help`.
- `#[command]`: `after_help` becomes `after_long_help`, so the performance note shows only with `--help`.

`tests/integration.rs`: new test: `-h` output has fewer lines than `--help` output, and each argument entry in `-h` is one line.

## Expected outcome
- `seqcol -h` fits on one screen: one line for each argument.
- `seqcol --help` prints the same details as now.

## Non-goals
- A man page (`clap_mangen`): see `IDEAS.md`.
- Changes to the details text.
