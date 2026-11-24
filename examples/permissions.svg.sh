#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
cargo run -qr -- --colors true permissions.txt -e '^[-dl]\S+' -S permissions.toml -s contrast -M underline |
    ./ansi2svg.sh > permissions.svg
