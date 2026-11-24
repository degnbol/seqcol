#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true ebola_alignment.faa -S hydrophobicity -s contrast -s gaps -c underline |
    ./ansi2svg.sh > alignment.svg
