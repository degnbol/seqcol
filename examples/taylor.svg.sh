#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true -s contrast -s gaps -S taylor ebola_alignment.faa |
    ./ansi2svg.sh > taylor.svg
