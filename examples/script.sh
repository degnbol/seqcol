#!/usr/bin/env bash
# Back up the data folder to a dated archive.
set -euo pipefail
src=${1:-data}  # folder to back up
dest=backups/$(date +%F).tar.gz
mkdir -p backups
# Skip temporary files.
tar --exclude='*.tmp' -czf "$dest" "$src"
echo "Wrote $dest"  # 1 archive per day
