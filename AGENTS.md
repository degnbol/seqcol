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

- `run` buffers its output. `write_line` writes every line except transposed rows: it batches consecutive same-styled characters and tracks the current style to minimize escape sequences
- Color mode hierarchy: truecolor > 256-color > 16-color (controlled by `--colors` flag or COLORTERM/TERM env vars)
- Output size limits per mode: `test_output_size` in `tests/integration.rs`
- Streaming mode used when possible (no transpose, no consensus)
- Transpose only affects sequence lines, not headers
