#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true -s hash.toml script.sh |
    ./ansi2svg.sh > hash.svg
