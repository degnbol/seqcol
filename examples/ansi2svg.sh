#!/usr/bin/env zsh
# Render ANSI text from stdin to SVG on stdout, on a light background.
# Optional argument: the maximum number of columns (default 114). Longer lines are cut to it.
# The image is as wide as the longest line, and the text size is the same for each width.
# Requires `cargo install term-transcript-cli --version 0.5.0` (the --styles selectors match its template)
# and FiraCode Nerd Font Mono (the width and line height come from its advance).
set -euo pipefail
max_cols=${1:-114}
family='FiraCode Nerd Font Mono'
# fc-match falls back to another font if the family is missing.
[[ $(fc-match -f '%{family[0]}' $family) == $family ]] || { print -u2 "$family is not installed"; exit 1 }
font=$(fc-match -f '%{file}' "${family}:style=Regular")
# Escape codes take no column.
text=$(perl -CS -pe "s/^((?:(?:\e\[[0-9;:]*m)*[^\e\n]){$max_cols}).*/\$1\e[0m/")
n_cols=$(print -r -- $text | perl -CS -ne 's/\e\[[0-9;:]*m//g; chomp; $n = length if length > $n; END { print $n + 0 }')
print -r -- $text |
term-transcript capture seqcol --pure-svg --no-inputs --no-wrap -p xterm \
    --width $(( 10 + n_cols * 862 / 100 )) --line-height 16.45px --embed-font $font \
    --styles 'svg > rect:first-of-type { fill: #faf4ed !important; } .container.fg7 { fill: #575279; } text { font-variant-ligatures: none; }'
