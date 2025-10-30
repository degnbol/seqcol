# seqcol

Colourise biological sequences (amino acids, DNA, and RNA).
Useful for viewing fasta files, sequence alignments, CSV, TSV, and other text files.
A simple commandline tool like `cat`.

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

