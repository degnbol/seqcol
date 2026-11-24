#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
seqcol() { cargo run -qr -- --colors true "$@" }
seqcol -e GAATTC -e GGATCC -e AAGCTT pUC19.fna |
    seqcol -e GAATTC -S '#fff59d' |
    seqcol -e GGATCC -S '#e1bee7' |
    seqcol -e AAGCTT -S '#b2ebf2' |
    ./ansi2svg.sh > restriction_sites.svg
