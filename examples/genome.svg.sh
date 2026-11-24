#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true -e GAATTC genome.fna |
    ./ansi2svg.sh > genome.svg
