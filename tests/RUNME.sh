#!/usr/bin/env zsh
cd $0:h

cargo run -- -S ./data/custom.csv ./data/custom.fa | less

grep -v '^>' ./data/aln_picorna.fa1  | ../target/debug/seqcol -Ts helix_propensity_aa | less

