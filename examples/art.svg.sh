#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true -S bits_rgb -H ^ art.txt |
    ./ansi2svg.sh > art.svg
