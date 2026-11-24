#!/usr/bin/env zsh
# Render ANSI text from stdin to SVG on stdout: 114 columns, light background.
# Requires `cargo install term-transcript-cli --version 0.5.0` (the --styles selectors match its template)
# and FiraCode Nerd Font Mono (the width and line height come from its advance).
set -euo pipefail
family='FiraCode Nerd Font Mono'
# fc-match falls back to another font if the family is missing.
[[ $(fc-match -f '%{family[0]}' $family) == $family ]] || { print -u2 "$family is not installed"; exit 1 }
font=$(fc-match -f '%{file}' "${family}:style=Regular")
term-transcript capture seqcol --pure-svg --no-inputs --no-wrap -p xterm \
    --width 992 --line-height 16.45px --embed-font $font \
    --styles 'svg > rect:first-of-type { fill: #faf4ed !important; } .container.fg7 { fill: #575279; } text { font-variant-ligatures: none; }'
