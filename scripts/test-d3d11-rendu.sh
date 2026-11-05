#!/bin/sh
# Rendu Direct3D 11 réel (#137) : testc/msvc/d3d11_msvc.c assemble un vertex
# et un pixel shader DXBC, dessine un quad texturé (tampons de sommets,
# d'index et de constantes, échantillonneur ponctuel, élimination des faces
# arrière) puis relit les pixels. Il faut un affichage OpenGL : DISPLAY, ou
# xvfb-run avec Mesa. Sauté (code 0) quand le runtime, clang ou OpenGL manque.
# Argument « hlsl » : testc/msvc/hlsl_msvc.c, mêmes vérifications avec des
# shaders compilés depuis du HLSL source par D3DCompile (#463).
set -eu
nom=${1:-d3d11}

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/${nom}_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/${nom}_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

display=""
if test -z "${DISPLAY:-}"; then
    command -v xvfb-run >/dev/null 2>&1 || { echo "ni DISPLAY ni xvfb-run : test sauté"; exit 0; }
    display=xvfb
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
set +e
if test -n "$display"; then
    HOME="$work" WINLUX_HOME="$work" WINLUX_DRIVE_C="$work" WINLUX_GUI=headless \
        timeout 120 xvfb-run -a -s '-screen 0 640x480x24' "$runtime" "$exe" >"$work/out.txt" 2>&1
else
    HOME="$work" WINLUX_HOME="$work" WINLUX_DRIVE_C="$work" WINLUX_GUI=headless \
        timeout 120 "$runtime" "$exe" >"$work/out.txt" 2>&1
fi
rc=$?
set -e
if test "$rc" -eq 77; then
    echo "$nom-rendu : pas de contexte OpenGL, test sauté"
    exit 0
fi
if test "$rc" -ne 0; then
    cat "$work/out.txt"
    echo "$nom-rendu : échec, code $rc (voir testc/msvc/${nom}_msvc.c)" >&2
    exit 1
fi
if test "$nom" = hlsl; then
    echo "hlsl-rendu : shaders HLSL compilés par D3DCompile, quad texturé, pixels relus"
else
    echo "d3d11-rendu : shaders DXBC traduits, quad texturé, pixels relus"
fi
