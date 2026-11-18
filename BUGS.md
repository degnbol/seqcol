# Bugs

Release blockers. Move to GitHub issues after the first release.

## Wrong output

- **DNA with `N` or IUPAC codes is detected as amino acids.**
  `AA_ONLY_CHARS` (`src/bio.rs`) contains `N` and `R Y K M S W B D H V`.
  Repro: `printf '>a\nACGTNN\n' > n.fa; seqcol -p never -s taylor n.fa` uses `taylor_aa`.
  Fix: classify by fraction of ACGTUN letters.
- **Lowercase sequence is not colored.**
  Builtin schemes and `-a` alphabets are uppercase only (soft-masked DNA, lowercase alignment output).
- **ANSI input is treated as text.** Escape bytes are matched and styled, which corrupts the codes,
  and the leading `\e[0m` makes header lines pass the default `-r`.
  Repro: `printf '>a\nACGT\n' | seqcol -p never -s taylor_nucl | seqcol -p never -s taylor_nucl | cat -v`.
  Fix: parse SGR codes at input, match on plain text, keep existing styles outside new matches.
- **FASTQ quality lines are colored.** Default `-r` excludes only lines starting with `>`, `@`, `+`.
  Fix: color only lines where every character is in the alphabet (given or detected).

## Crashes and lost input

- **Base scheme names fail on stdin.** Auto-detection skips stdin, so the name falls through to a file read.
  Repro: `printf '>a\nACGT\n' | seqcol -p never -s taylor` panics with "taylor: No such file".
- **Process substitution input is lost.** Auto-detection reads the FIFO before the main read.
  Repro: `seqcol -p never -s taylor <(printf '>a\nACGT\n')` prints nothing.
- **Non-ASCII input panics.** `ansi_byte` (`src/ansi_colors.rs`) encodes a `char` into one byte.
  Repro: `printf 'ACé-\n' | seqcol -p never -s taylor_nucl`.
- **Invalid UTF-8 input panics** (`Result::unwrap` in `src/inout.rs`).
## Error handling

- A closed pipe gives exit 0, also when an input file failed to open.
  `main` (`src/main.rs`) maps `BrokenPipe` to success, so `run` never returns the failure.
  Repro: `seqcol -p never missing.fa big.fa | head -1; echo $pipestatus`.
- User errors use `panic!`/`expect` and print "thread 'main' panicked":
  unknown scheme, `-i` char without bg color, unknown `-a` keyword, invalid `-r`.
  Return `anyhow` errors instead.

## Minor

- `TERM=dumb` gives 16 colors instead of none; `NO_COLOR` is not respected.
