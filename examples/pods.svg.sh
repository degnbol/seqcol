#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true pods.txt -e '\S+$' -M underline |
    ./ansi2svg.sh > pods.svg
