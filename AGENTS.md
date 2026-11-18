# Architecture

- `src/main.rs` - CLI argument parsing (clap) to `Config`, pager, exit code
- `src/lib.rs` - `Config`, and `run`: styled output from readers to a writer
- `src/ansi_colors.rs` - ANSI escape code handling, color mode conversions (truecolor/256/16), optimized `write_line` with state tracking
- `src/colorschemes.rs` - Builtin colorscheme loading (from `data/colorschemes/`), custom file parsing
- `src/consensus.rs` - Consensus/mutation highlighting logic
- `src/matches.rs` - Regex match ranges (`-r`, `-m`)
- `src/bio.rs` - Alphabet definitions (DNA, RNA, AA), sequence type detection
- `src/inout.rs` - File I/O utilities
- `src/pager.rs` - Pager process

# Performance Notes

- `write_line` batches consecutive same-styled characters and tracks fg/bg state to minimize escape sequences
- Color mode hierarchy: truecolor > 256-color > 16-color (controlled by `--colors` flag or COLORTERM/TERM env vars)
- Output sizes (example file): truecolor ~48KB, 256-color ~49KB, 16-color ~23KB; `-S hydrophobicity_aa -c gray` ~42KB, `-c bold` ~39KB
- Streaming mode used when possible (no transpose, no consensus)
- Transpose only affects sequence lines, not headers

# Test color modes

```zsh
./target/release/seqcol -p never file.fa -s hydrophobicity_aa --colors=16 | wc -c
```
