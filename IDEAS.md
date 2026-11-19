
- New arg for consensus to set a threshold, to only highlight if more than X percent of seqs agree at a given position.
- New arg for consensus to ignore certain characters, specifically gaps.

A reverse flag (the inverse ANSI code) or some way of saying e.g. show gaps in reverse primary.

Add gradient support, e.g. from builtin list of properties for each AA, etc. or from a supplied list of numbers.

Motif scoring from a file of per-position letter probabilities (a position weight matrix, as in a sequence logo), e.g. a TATA box.
Highlight each window with a gradient from poor to good match.

Option for highlighting problems, e.g. highlighting X in sequences, yet having the full alphabet defined so the -m flag still works for filtering when X is in a sequence.
- Highlight problems/ambiguous characters (X, N, etc.) with a `-w/--warn` flag

Edit an `-a` keyword, e.g. `-a dna,^N` removes N and `-a aa,X` adds X.
It could replace the " no gap" keywords (`-a dna,^-`).

Separate include and exclude patterns, so `-r` (include) no longer replaces the header exclusion.
- Exclude defaults to FASTA/FASTQ headers; possibly more heuristics, e.g. the header row of a TSV.
- Combine highlights by piping seqcol into seqcol (each stage layers its styles) instead of nesting or AND flags.

- Line numbers / position markers for alignments
- Reference sequence comparison (highlight differences from first sequence, not consensus)
- Conservation scores (color by column conservation percentage)
- Gap-only column filtering (hide or mark all-gap columns)
- Sequence name labels alongside transposed output

Run-time regression checks (`test_output_size` only checks output size):
- `criterion` benchmark of `seqcol::run`: real time, but noisy, so compare by hand against a saved baseline.
- `iai-callgrind`: deterministic instruction counts, so it can be a pass/fail gate in Linux CI.
  A proxy for time: kernel time in syscalls and real cache behaviour are not measured. Needs valgrind, so not on macOS ARM.

This tool is somewhat like grep. We could take closer inspiration for ease of 
use, e.g. `-v` for invert (regex match), and `-A` and `-B` to only show some 
lines of context around matches, rather than the whole file. For large files it 
might be a use-case to only see motif matches with some context, rather than 
the entire file (this is data after all).

Partial args: `-s turn` should be valid. Error if a prefix matches more than one scheme, except a full name (`-s jalview` vs `jalview_ambiguity`).

Select one type section of a scheme, e.g. `-s nucl:taylor -s aa:zappo`.

A man page could be generated from the clap definitions (`clap_mangen`).

