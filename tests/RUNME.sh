#!/usr/bin/env zsh
cd $0:h

cargo run -- -s ./data/custom.csv ./data/custom.fa | less -RS~#8

grep -v '^>' ./data/aln_picorna.fa1  | ../target/debug/seqcol -Ts helix_propensity_aa | less -RS~#8

../target/debug/seqcol ./data/ebola_virus_reduced_align.fa1 -s '' -S hydrophobicity_aa -c '128 128 128' | less -RS~#8
