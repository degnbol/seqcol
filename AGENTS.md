# Architecture

- `src/main.rs` - CLI argument parsing (clap), pager handling, main coloring logic
- `src/ansi_colors.rs` - ANSI escape code handling, color mode conversions (truecolor/256/16), optimized `write_line` with state tracking
- `src/colorschemes.rs` - Builtin colorscheme loading (from `data/colorschemes/`), custom file parsing
- `src/consensus.rs` - Consensus/mutation highlighting logic
- `src/bio.rs` - Alphabet definitions (DNA, RNA, AA)
- `src/inout.rs` - File I/O utilities

# Performance Notes

- `write_line` batches consecutive same-styled characters and tracks fg/bg state to minimize escape sequences
- Color mode hierarchy: truecolor > 256-color > 16-color (controlled by `--colors` flag or COLORTERM/TERM env vars)
- Output sizes (example file): truecolor ~48KB, 256-color ~49KB, 16-color ~23KB
- Streaming mode used when possible (no transpose, no consensus)
- Transpose only affects sequence lines, not headers

# Test color modes

```zsh
./target/release/seqcol -p never file.fa -s hydrophobicity_aa --colors=16 | wc -c
```
