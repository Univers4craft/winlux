#!/bin/sh
# Exécute sous Winlux les tests C compilés par scripts/build-mingw-tests.sh.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
dir=${WINLUX_MINGW_DIR:-$root/target/mingw}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

test -x "$runtime" || { echo "runtime absent : $runtime" >&2; exit 1; }
test -f "$dir/mingw_suite.exe" || { echo "lancez d'abord scripts/build-mingw-tests.sh" >&2; exit 1; }
cd "$work"
set +e
WINLUX_GUI=headless "$runtime" "$dir/mingw_suite.exe" un "deux mots" >"$work/sortie.txt" 2>&1
status=$?
set -e
cat "$work/sortie.txt"
test "$status" -eq 0 || { echo "mingw_suite.exe : code $status" >&2; exit 1; }
grep -q '^atexit exécuté$' "$work/sortie.txt" || { echo "les fonctions atexit n'ont pas été exécutées" >&2; exit 1; }

# Fibres des programmes 64 bits (#654) : ConvertThreadToFiber, CreateFiber(Ex),
# SwitchToFiber aller-retour, pile propre, fin de routine = fin du thread.
test -f "$dir/fiber_suite.exe" || { echo "fiber_suite.exe absent" >&2; exit 1; }
set +e
WINLUX_GUI=headless "$runtime" "$dir/fiber_suite.exe" >"$work/fibres.txt" 2>&1
fiber_status=$?
set -e
cat "$work/fibres.txt"
test "$fiber_status" -eq 0 || { echo "fiber_suite.exe : échec $fiber_status" >&2; exit 1; }
grep -q '^Fibres 64 bits : tout est correct$' "$work/fibres.txt" || { echo "fiber_suite : marqueur de réussite absent" >&2; exit 1; }

# Fonctions des vrais jeux (docs/VRAIS-JEUX.md), en msvcrt puis en UCRT.
for name in games_suite games_suite_ucrt; do
    test -f "$dir/$name.exe" || { echo "$name.exe absent" >&2; exit 1; }
    set +e
    WINLUX_GAMES_TEST=jeu WINLUX_GUI=headless "$runtime" "$dir/$name.exe" >"$work/$name.txt" 2>&1
    games_status=$?
    set -e
    cat "$work/$name.txt"
    test "$games_status" -eq 0 || { echo "$name.exe : $games_status test(s) en échec" >&2; exit 1; }
    grep -q 'tout est bon' "$work/$name.txt" || { echo "$name : marqueur de réussite absent" >&2; exit 1; }
done

if test -f "$dir/cpp_exceptions.exe"; then
    set +e
    WINLUX_GUI=headless "$runtime" "$dir/cpp_exceptions.exe" >"$work/cpp.txt" 2>&1
    cpp_status=$?
    set -e
    cat "$work/cpp.txt"
    if test "$cpp_status" -ne 0; then
        echo "cpp_exceptions.exe : code $cpp_status (déroulement SEH incomplet, non bloquant)" >&2
    elif ! grep -q 'all tests passed' "$work/cpp.txt"; then
        echo "cpp_exceptions : marqueur de réussite absent (non bloquant)" >&2
    fi
fi

if test -f "$dir/eh_msvc.exe"; then
    set +e
    WINLUX_GUI=headless "$runtime" "$dir/eh_msvc.exe" >"$work/msvc.txt" 2>&1
    msvc_status=$?
    set -e
    cat "$work/msvc.txt"
    test "$msvc_status" -eq 0 || { echo "eh_msvc.exe : le test n° $msvc_status a échoué" >&2; exit 1; }
    echo "eh_msvc : exceptions C++ et __try au format MSVC OK"
    # Sans case UnwindHelp (dispUnwindHelp = 0), comme certaines fonctions
    # MSVC : la relance « throw; » ne doit pas reboucler (#436).
    python3 -I - "$dir/eh_msvc.exe" "$work/eh_nohelp.exe" <<'PY'
import struct, sys
data = bytearray(open(sys.argv[1], "rb").read())
magic = struct.pack("<I", 0x19930522)
i = data.find(magic)
while i >= 0:
    struct.pack_into("<i", data, i + 28, 0)
    i = data.find(magic, i + 4)
open(sys.argv[2], "wb").write(data)
PY
    set +e
    WINLUX_GUI=headless "$runtime" "$work/eh_nohelp.exe" >"$work/msvc-nohelp.txt" 2>&1
    nohelp_status=$?
    set -e
    test "$nohelp_status" -eq 0 || { cat "$work/msvc-nohelp.txt"; echo "eh_msvc sans UnwindHelp : le test n° $nohelp_status a échoué" >&2; exit 1; }
    echo "eh_msvc sans UnwindHelp : relances OK"
fi

# Fautes matérielles en exceptions SEH (#269), bloquant.
if test -f "$dir/hwexc_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-hw-exceptions.sh"
fi

# Manettes et périphériques HID (#271), bloquant.
if test -f "$dir/hid_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-hid.sh"
fi

# DeviceIoControl sur volumes, disques et fichiers creux (#271), bloquant.
if test -f "$dir/devio_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-devio.sh"
fi

# Annulation des E/S, handles et tampons invalides (#18), bloquant.
if test -f "$dir/io_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-io-cancel.sh"
fi

# Entrée brute : souris relative et clavier en WM_INPUT (#271), bloquant.
if test -f "$dir/rawinput_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-rawinput.sh"
fi

# Ports série : recherche du port, réglage, WaitCommEvent (#271), bloquant.
if test -f "$dir/serial_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-serial.sh"
fi

# Arbre des périphériques : ports COM comme pyserial, cfgmgr32 (#271), bloquant.
if test -f "$dir/devnodes_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-devnodes.sh"
fi

# Héritage des handles entre processus (crashpad), bloquant.
if test -f "$dir/inherit_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-heritage.sh"
fi
# DuplicateHandle et mémoire entre processus, enfant suspendu (#236), bloquant.
if test -f "$dir/dupx_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-duplication.sh"
fi
# WebView2 maison sur WebKitGTK (#236), bloquant (sauté sans WebKitGTK).
if test -f "$dir/webview2_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-webview2.sh"
fi
# Fonctions vérifiées au démarrage par Chromium et Electron (#236), bloquant.
if test -f "$dir/chromium_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-chromium-api.sh"
fi
# Rendu Direct3D 11 : shaders DXBC traduits en GLSL (#137), bloquant.
if test -f "$dir/d3d11_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-d3d11-rendu.sh"
fi
# Rendu Direct3D 11 depuis du HLSL source compilé par D3DCompile (#463), bloquant.
if test -f "$dir/hlsl_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-d3d11-rendu.sh" hlsl
fi
# winusb : appareil USB trouvé par setupapi, décrit par WinUSB (#271), bloquant.
if test -f "$dir/winusb_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-winusb.sh"
fi

# Définition d'écran des jeux, remise à la sortie (#271), bloquant.
if test -f "$dir/display_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-display-mode.sh"
fi

# GDI+ en API plate : dessin, texte, LockBits, PNG, HBITMAP (#493), bloquant.
if test -f "$dir/gdiplus_suite.exe"; then
    set +e
    WINLUX_GUI=headless "$runtime" "$dir/gdiplus_suite.exe" >"$work/gdiplus.txt" 2>&1
    gdiplus_status=$?
    set -e
    cat "$work/gdiplus.txt"
    test "$gdiplus_status" -eq 0 || { echo "gdiplus_suite.exe : le test n° $gdiplus_status a échoué" >&2; exit 1; }
fi

echo "Tests MinGW réussis."
