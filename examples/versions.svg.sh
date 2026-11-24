#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true versions.txt -e '[0-9.]+$' -M underline |
    ./ansi2svg.sh > versions.svg
