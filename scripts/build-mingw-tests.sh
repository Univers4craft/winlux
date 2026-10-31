#!/bin/sh
# Compile les tests C et C++ avec la chaîne MinGW-w64 :
# ce sont de vrais exécutables Windows produits par un compilateur tiers.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cc=${WINLUX_MINGW_CC:-x86_64-w64-mingw32-gcc}
cxx=${WINLUX_MINGW_CXX:-x86_64-w64-mingw32-g++}
out="$root/target/mingw"
command -v "$cc" >/dev/null 2>&1 || { echo "$cc absent : installez gcc-mingw-w64-x86-64" >&2; exit 1; }
mkdir -p "$out"
for src in "$root"/testc/*.c; do
    name=$(basename "$src" .c)
    "$cc" -O2 -Wall -Werror -o "$out/$name.exe" "$src"
done
if command -v "$cxx" >/dev/null 2>&1; then
    for src in "$root"/testc/*.cpp; do
        test -f "$src" || continue
        name=$(basename "$src" .cpp)
        "$cxx" -O2 -Wall -Werror -o "$out/$name.exe" "$src"
    done
fi
echo "Tests MinGW construits dans $out"
