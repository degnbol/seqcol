#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
# Cut after seqcol, since the consensus needs the whole columns. Escape codes take no column.
cargo run -qr -- --colors true ebola_alignment.faa -S hydrophobicity -s contrast -s gaps -c underline |
    perl -pe 's/^((?:(?:\e\[[0-9;:]*m)*[^\e\n]){114}).*/$1\e[0m/' |
    ./ansi2svg.sh > alignment.svg
