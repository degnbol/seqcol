# Bugs

Release blockers. Move to GitHub issues after the first release.

## Wrong output

- **Lowercase sequence is not colored.**
  Builtin schemes and `-a` alphabets are uppercase only (soft-masked DNA, lowercase alignment output).
- **ANSI input is treated as text.** Escape bytes are matched and styled, which corrupts the codes,
  and the leading `\e[0m` makes header lines pass the default `-r`.
  Repro: `printf '>a\nACGT\n' | seqcol -p never -s taylor | seqcol -p never -s taylor | cat -v`.
  Fix: parse SGR codes at input, match on plain text, keep existing styles outside new matches.
- **FASTQ quality lines are colored.** Default `-r` excludes only lines starting with `>`, `@`, `+`.
  Their amino acid evidence can also make a read of 1 or 2 letters amino acids.
  Fix: color only lines where every character is in the alphabet.
- **`.` gaps split `-m` runs.** Stockholm and A2M alignments use `.` as a gap, but no alphabet has it,
  so with `-m` the parts of a sequence between gaps can be too short to color.

## Minor

- `TERM=dumb` gives 16 colors instead of none; `NO_COLOR` is not respected.
