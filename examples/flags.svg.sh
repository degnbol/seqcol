#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true flags.txt -e '[01]{16}$' -S flags.toml -s contrast -M underline |
    ./ansi2svg.sh > flags.svg
