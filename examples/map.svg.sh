#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true map.txt -S map.toml |
    ./ansi2svg.sh > map.svg
