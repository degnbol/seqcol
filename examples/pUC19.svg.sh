#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true -e GAATTC pUC19.fna |
    ./ansi2svg.sh > pUC19.svg
