#!/bin/sh
# Dossier système des installateurs 32 bits (#1800, #1796) : un vrai
# installateur NSIS x86, fabriqué ici par makensis, dépose une DLL de test
# dans $SYSDIR. Vérifie :
#   - sans rien de plus, elle arrive dans C:\Windows\SysWOW64, jamais dans
#     System32 (comme la redirection WOW64 de Windows 64 bits) ;
#   - après ${DisableX64FSRedirection}, elle arrive dans System32, puis
#     ${EnableX64FSRedirection} rétablit SysWOW64.
# La source Inno Setup équivalente (scripts/testdata/sysdir-32bits/inno-sys.iss)
# sert au banc : ISCC n'existe pas sous Linux.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
command -v makensis >/dev/null || { echo "makensis absent (paquet nsis) : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$root/scripts/testdata/sysdir-32bits/nsis-sysdir.nsi" "$work/"
cd "$work"
echo "DLL de test x86 de Winlux (#1800)" >winlux-test-x86.dll
echo "DLL de test native de Winlux (#1800)" >winlux-test-native.dll
echo "DLL de test après rétablissement (#1800)" >winlux-test-retour.dll
makensis -V1 nsis-sysdir.nsi >/dev/null

mkdir home
fail() { echo "$1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_SETUP_RUN=0 \
    "$runtime" --setup sysdir-test.exe >"$work/out.txt" 2>&1 || fail "installation NSIS en échec"

where() { find "$work/home" -iname "$1" -printf '%P\n' | tr 'A-Z' 'a-z'; }
case "$(where winlux-test-x86.dll)" in
    windows/syswow64/*) ;;
    *) fail "\$SYSDIR d'un installateur x86 : « $(where winlux-test-x86.dll) » au lieu de windows/syswow64" ;;
esac
case "$(where winlux-test-native.dll)" in
    windows/system32/*) ;;
    *) fail "redirection coupée : « $(where winlux-test-native.dll) » au lieu de windows/system32" ;;
esac
case "$(where winlux-test-retour.dll)" in
    windows/syswow64/*) ;;
    *) fail "redirection rétablie : « $(where winlux-test-retour.dll) » au lieu de windows/syswow64" ;;
esac
echo "OK : \$SYSDIR d'un installateur NSIS x86 dans SysWOW64, System32 seulement redirection coupée (#1800)"
