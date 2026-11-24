#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true -c bold ebola_alignment.faa |
    ./ansi2svg.sh > consensus.svg
