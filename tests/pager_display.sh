#!/usr/bin/env zsh
# Navigate seqcol's pager in tmux, and after each step compare the screen with a reference.
# The reference is seqcol's pager on the finished output, written unchanged
# (a -e that never matches), and given the same keys.
# Usage: tests/pager_display.sh [STEP...]
#   STEP: tmux keys separated by spaces, e.g. 'G' 'b b' '3 0 0 g' 'Right Right'
# Needs tmux and less. Exit code 1 if a screen differs.
set -euo pipefail
cd ${0:A:h}
ROOT=$(git root)
cargo build -q --release
seqcol=$ROOT/target/release/seqcol
steps=(G 'b b' 'k k k' 'Right Right Right' u Left '3 0 0 g' 'd d' g Right G)
(( $# )) && steps=("$@")

dir=$(mktemp -d)
awk 'BEGIN { srand(1); for (i = 0; i < 300; i++) {
    printf ">seq%d sample\n", i
    for (j = 0; j < 200; j++) printf "%s", substr("ACGT", int(rand() * 4) + 1, 1)
    print "" } }' > $dir/in.fa
COLORTERM=truecolor $seqcol -p never -s taylor $dir/in.fa > $dir/out.ans

T=(tmux -L seqcol-pager-display -f /dev/null)
trap 'rc=$?; $T kill-server 2>/dev/null; exit $rc' EXIT
# A first session starts the server, so that panes stay after their program ends.
$T new-session -d -s server
$T set-option -g remain-on-exit on

# The screen, after it stops changing. Fails if the pager ended or shows nothing.
capture() {
    local prev= cur
    for _ in {1..50}; do
        if [[ $($T display-message -p -t $1 '#{pane_dead}') == 1 ]]; then
            print -u2 "$1: pager ended: $($T capture-pane -p -t $1 | grep -v '^$')"
            return 1
        fi
        cur=$($T capture-pane -p -e -t $1)
        if [[ -n ${cur//[[:space:]]/} && $cur == $prev ]]; then
            print -r -- $cur
            return 0
        fi
        prev=$cur
        sleep 0.1
    done
    print -u2 "$1: screen is empty or does not settle"
    return 1
}

# The user's LESS and PAGER, e.g. from ~/.zshenv, must not apply.
pager() { $T new-session -d -s $1 -x 100 -y 30 "unset LESS PAGER; COLORTERM=truecolor $seqcol -p always ${(j: :)@[2,-1]}" }
pager tested -s taylor $dir/in.fa
pager reference -s taylor -e "'a^'" $dir/out.ans

failed=0
for step in $steps; do
    for session in tested reference; do
        capture $session > /dev/null
        $T send-keys -t $session ${=step}
    done
    # Else the screens can be captured before the keys arrive.
    sleep 0.2
    tested_screen=$(capture tested)
    reference_screen=$(capture reference)
    if [[ $tested_screen == $reference_screen ]]; then
        print -r -- "ok    '$step'"
    else
        print -r -- $tested_screen > $dir/tested.${step:gs/ /_/}
        print -r -- $reference_screen > $dir/reference.${step:gs/ /_/}
        print -r -- "FAIL  '$step': diff $dir/{tested,reference}.${step:gs/ /_/}"
        failed=1
    fi
done
(( failed )) || rm -r $dir
exit $failed
