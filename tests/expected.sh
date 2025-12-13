#!/usr/bin/env zsh
cd $0:h

COLORTERM=truecolor ../target/release/seqcol -s ./data/custom.csv ./data/custom.fa > ./expected/custom_colorscheme.txt

grep -v '^>' ./data/aln_picorna.fa1  | COLORTERM=truecolor ../target/release/seqcol -Ts helix_propensity_aa > ./expected/transpose_helix_propensity.txt

COLORTERM=truecolor ../target/release/seqcol ./data/ebola_virus_reduced_align.fa1 -s '' -S hydrophobicity_aa -c '128 128 128' > ./expected/foreground_consensus.txt

COLORTERM=truecolor ../target/release/seqcol ./data/ebola_virus_reduced_align.fa1 -s '' -S hydrophobicity_aa -C '128 128 128' > ./expected/foreground_mutations.txt

