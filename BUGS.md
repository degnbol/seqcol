# Bugs

Release blockers. Move to GitHub issues after the first release.

## Wrong output

- **Lowercase sequence is not colored.**
  Builtin schemes and `-a` alphabets are uppercase only (soft-masked DNA, lowercase alignment output).
- **FASTQ quality lines are colored.** Default `-r` excludes only lines starting with `>`, `@`, `+`.
  Their amino acid evidence can also make a read of 1 or 2 letters amino acids.
  Fix: color only lines where every character is in the alphabet.
