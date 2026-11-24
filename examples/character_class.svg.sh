#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true -s character_class -S black script.sh |
    ./ansi2svg.sh > character_class.svg
