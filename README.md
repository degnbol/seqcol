# seqcol

Colourise biological sequences (amino acids, DNA, and RNA).
Useful for viewing fasta files, sequence alignments, CSV, TSV, and other text files.
A simple commandline tool like `cat`, which may be useful for colourising 
sequence of characters in general.

![Colourising demo with a few fasta records.](https://github.com/degnbol/seqcol/blob/main/data/demo.png?raw=true)

## Building

Build requires Rust and Cargo
https://doc.rust-lang.org/cargo/getting-started/installation.html

```
cargo build --release
```

The binary should then be available:
```
./target/release/seqcol --help
```

Example use producing the demo image above:
```
./target/release/seqcol ./tests/data/ebola_virus_reduced_align.fa1 -S hydrophobicity_aa -c gray
```

## Performance

For long sequences, horizontal scrolling in a pager can be slow due to ANSI color codes.
Reduce output size by forcing a simpler color mode:
```
seqcol file.fa -s hydrophobicity_aa --colors=256  # ~40% smaller
seqcol file.fa -s hydrophobicity_aa --colors=16   # ~50% smaller
```
Or consider transposing with flag `-T`.

