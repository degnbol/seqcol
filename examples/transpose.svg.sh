#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
# The first 40 lines: the 7 headers and 33 columns.
cargo run -qr -- --colors true -T ebola_alignment.faa | sed -n 1,40p |
    ./ansi2svg.sh > transpose.svg
