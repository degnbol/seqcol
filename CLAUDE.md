# Claude Code Notes

## Architecture

- `src/main.rs` - CLI argument parsing (clap), pager handling, main coloring logic
- `src/ansi_colors.rs` - ANSI escape code handling, color mode conversions (truecolor/256/16), optimized `write_line` with state tracking
- `src/colorschemes.rs` - Builtin colorscheme loading (from `data/colorschemes/`), custom file parsing
- `src/consensus.rs` - Consensus/mutation highlighting logic
- `src/bio.rs` - Alphabet definitions (DNA, RNA, AA)
- `src/inout.rs` - File I/O utilities

## Performance Notes

- `write_line` batches consecutive same-styled characters and tracks fg/bg state to minimize escape sequences
- Color mode hierarchy: truecolor > 256-color > 16-color (controlled by `--colors` flag or COLORTERM/TERM env vars)
- Output sizes (example file): truecolor ~48KB, 256-color ~49KB, 16-color ~23KB
- Streaming mode used when possible (no transpose, no consensus)
- Transpose only affects sequence lines, not headers

## Code Conventions

- Use `write_all` not `write` for proper IO handling
- Clippy clean with default lints
- Integration tests in `tests/integration.rs`

## Future Feature Ideas

### From TODO.md
- Auto-detect alphabet (DNA/RNA/AA) from sequence content
- Highlight problems/ambiguous characters (X, N, etc.) with a `-w/--warn` flag
- Separate include/exclude regex patterns
- Reverse/inverse ANSI codes for gaps

### Additional ideas for bioinformaticians
- Line numbers / position markers for alignments
- Reference sequence comparison (highlight differences from first sequence, not consensus)
- Conservation scores (color by column conservation percentage)
- Gap-only column filtering (hide or mark all-gap columns)
- Sequence name labels alongside transposed output

## Testing

```bash
cargo test                    # Run all tests
cargo clippy                  # Check lints
cargo build --release         # Build optimized binary

# Test color modes
./target/release/seqcol -p never file.fa -s hydrophobicity_aa --colors=16 | wc -c
```

## Recent Changes (Dec 2024)

- Added `--colors`/`-k` flag for explicit color mode control (16/256/true)
- Extracted consensus logic to `src/consensus.rs`
- Optimized ANSI output with state tracking in `write_line`
- Added `ansi16()` for basic 16-color mapping
- Transpose now only affects sequence lines (headers printed normally first)
- Added colorscheme descriptions to `-l` output with word wrapping
- Fixed all clippy warnings
