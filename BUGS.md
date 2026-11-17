# Bugs

Release blockers. Move to GitHub issues after the first release.

## Wrong output

- **`-c`/`-C` with `bold` or `underline` has no effect** (except with `-T`).
  `write_line` (`src/ansi_colors.rs`) writes only fg/bg codes and drops other attributes.
  Repro: `printf 'ACGT\nACGA\nACGA\n' | seqcol -p never -s taylor_nucl -c bold | cat -v` has no `\e[1m`.
- **DNA with `N` or IUPAC codes is detected as amino acids.**
  `AA_ONLY_CHARS` (`src/bio.rs`) contains `N` and `R Y K M S W B D H V`.
  Repro: `printf '>a\nACGTNN\n' > n.fa; seqcol -p never -s taylor n.fa` uses `taylor_aa`.
  Fix: classify by fraction of ACGTUN letters.
- **Wrapped FASTA gives wrong `-c`, `-C`, `-T` output.**
  Positions are line columns, not record positions, so wrapped lines of one record are compared as separate sequences.
  Open decision: join wrapped lines per record, or document one-line FASTA and warn on unequal line lengths.
- **Lowercase sequence is not colored.**
  Builtin schemes and `-a` alphabets are uppercase only (soft-masked DNA, lowercase alignment output).
- **FASTQ quality lines are colored.** Default `-r` excludes only lines starting with `>`, `@`, `+`.

## Crashes and lost input

- **`-r` with `-m` panics when the `-r` match does not start at column 0.**
  Inner regex offsets are relative to the outer match but are used as line offsets (`src/main.rs`, both "2 regexes" branches).
  Repro: `printf 'xxxxxxxxACGTACGT\n' | seqcol -p never -s taylor_nucl -r 'ACGT.*' -m 3`.
- **Base scheme names fail on stdin.** Auto-detection skips stdin, so the name falls through to a file read.
  Repro: `printf '>a\nACGT\n' | seqcol -p never -s taylor` panics with "taylor: No such file".
- **Process substitution input is lost.** Auto-detection reads the FIFO before the main read.
  Repro: `seqcol -p never -s taylor <(printf '>a\nACGT\n')` prints nothing.
- **Invalid UTF-8 input panics** (`Result::unwrap` in `src/inout.rs`).

## Error handling

- Missing input file exits 0 (`src/inout.rs`).
- User errors use `panic!`/`expect` and print "thread 'main' panicked":
  unknown scheme, `-i` char without bg color, unknown `-a` keyword, invalid `-r`, invalid consensus color.
  Return `anyhow` errors instead.

## Minor

- `-p always` is documented as "without auto-quit", but `spawn_pager` always adds `-F`.
  The `-p` help text lists different less flags than the code adds.
- Version is hard-coded in `#[command(version = "0.1.0")]`; use `#[command(version)]` to read `Cargo.toml`.
- Unused dependencies: `termbg`, `terminal-colorsaurus`.
- `TERM=dumb` gives 16 colors instead of none; `NO_COLOR` is not respected.
