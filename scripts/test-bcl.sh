#!/bin/bash
# Témoins de la BCL Winlux en C# (étape 7) : chaque bcl/temoins/X.cs est
# compilé contre notre mscorlib, puis lancé par winlux ; son code de sortie
# doit valoir celui annoncé par la ligne « Code de sortie attendu : N. ».
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
winlux="${WINLUX_BIN:-$root/target/release/winlux}"
"$root/scripts/build-bcl.sh" --temoins

failures=0
for src in "$root"/bcl/temoins/*.cs; do
    name="$(basename "$src" .cs)"
    expected="$(sed -n 's/.*Code de sortie attendu : \([0-9]*\)\..*/\1/p' "$src" | head -n 1)"
    set +e
    (cd "$root/build/bcl-temoins" && WINLUX_BCL_DIR="$root/build/bcl" timeout 60 "$winlux" "$name.exe" >/dev/null)
    code=$?
    set -e
    if [ "$code" = "$expected" ]; then
        echo "ok   $name : code $code"
    else
        echo "ÉCHEC $name : code $code, attendu $expected"
        failures=$((failures + 1))
    fi
done
exit $((failures > 0))
