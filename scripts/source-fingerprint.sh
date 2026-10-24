#!/bin/sh
# Fingerprint the shipped sources, including new files not committed yet.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
{
    for directory in runtime testexe testdll integration scripts .cargo; do
        test ! -d "$directory" || find "$directory" -type f
    done
    printf '%s\n' Cargo.toml Cargo.lock
} | LC_ALL=C sort | while IFS= read -r file; do
    test ! -f "$file" || sha256sum "$file"
done | sha256sum | cut -d' ' -f1
