#!/usr/bin/env zsh
# The multiple cloning site of pUC19 (positions 396-452) on the first line of 70, with flanks.
set -euo pipefail
cd ${0:A:h}
curl -sSf 'https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=nuccore&id=L09137.2&rettype=fasta&retmode=text&seq_start=386&seq_stop=525' |
    sed '/^$/d' > pUC19.fna
