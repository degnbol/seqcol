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
./target/debug/seqcol ./tests/data/ebola_virus_reduced_align.fa1 -S hydrophobicity_aa -c gray | less -RS~#8
```

