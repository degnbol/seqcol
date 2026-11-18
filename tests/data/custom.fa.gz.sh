#!/usr/bin/env zsh
set -euo pipefail
cd ${0:A:h}
gzip -k -n -f custom.fa
