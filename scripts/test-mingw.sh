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

# Vrai service Windows Unicode (#17) : CreateServiceW, StartServiceW,
# StartServiceCtrlDispatcherW, RegisterServiceCtrlHandlerExW, arrêt, suppression.
test -f "$dir/service_suite.exe" || { echo "service_suite.exe absent" >&2; exit 1; }
mkdir -p "$work/service"
set +e
(cd "$work/service" && WINLUX_REGISTRY_PATH="$work/service/registre.tsv" WINLUX_GUI=headless \
    timeout 120 "$runtime" "$dir/service_suite.exe") >"$work/service.txt" 2>&1
service_status=$?
set -e
cat "$work/service.txt"
test "$service_status" -eq 0 || { echo "service_suite.exe : code $service_status" >&2; exit 1; }
grep -q '^Service Windows : tout est correct$' "$work/service.txt" || { echo "service_suite : marqueur de réussite absent" >&2; exit 1; }

# Activation COM depuis le registre (#16) : InprocServer32, DllGetClassObject,
# CoCreateInstanceEx, CoFreeUnusedLibraries.
test -f "$dir/com_suite.exe" || { echo "com_suite.exe absent" >&2; exit 1; }
test -f "$dir/com_server.dll" || { echo "com_server.dll absent" >&2; exit 1; }
mkdir -p "$work/com"
set +e
(cd "$work/com" && WINLUX_REGISTRY_PATH="$work/com/registre.tsv" WINLUX_GUI=headless \
    timeout 120 "$runtime" "$dir/com_suite.exe") >"$work/com.txt" 2>&1
com_status=$?
set -e
cat "$work/com.txt"
test "$com_status" -eq 0 || { echo "com_suite.exe : code $com_status" >&2; exit 1; }
grep -q '^COM par le registre : tout est correct$' "$work/com.txt" || { echo "com_suite : marqueur de réussite absent" >&2; exit 1; }

# OLE Automation (#16) : CreateDispTypeInfo, CreateStdDispatch, Invoke
# (conversions, arguments nommés, propriétés), DispCallFunc, objets actifs.
test -f "$dir/automation_suite.exe" || { echo "automation_suite.exe absent" >&2; exit 1; }
set +e
WINLUX_GUI=headless timeout 120 "$runtime" "$dir/automation_suite.exe" >"$work/automation.txt" 2>&1
automation_status=$?
set -e
cat "$work/automation.txt"
test "$automation_status" -eq 0 || { echo "automation_suite.exe : code $automation_status" >&2; exit 1; }
grep -q '^Automation OLE : tout est correct$' "$work/automation.txt" || { echo "automation_suite : marqueur de réussite absent" >&2; exit 1; }

# Serveur COM hors processus (#16) : LocalServer32 lancé avec -Embedding,
# CoRegisterClassObject(CLSCTX_LOCAL_SERVER), IDispatch entre processus.
test -f "$dir/com_local_suite.exe" || { echo "com_local_suite.exe absent" >&2; exit 1; }
mkdir -p "$work/com-local"
set +e
(cd "$work/com-local" && WINLUX_REGISTRY_PATH="$work/com-local/registre.tsv" WINLUX_GUI=headless \
    timeout 120 "$runtime" "$dir/com_local_suite.exe") >"$work/com-local.txt" 2>&1
com_local_status=$?
set -e
cat "$work/com-local.txt"
test "$com_local_status" -eq 0 || { echo "com_local_suite.exe : code $com_local_status" >&2; exit 1; }
grep -q '^COM hors processus : tout est correct$' "$work/com-local.txt" || { echo "com_local_suite : marqueur de réussite absent" >&2; exit 1; }

# MSXML intégré (#1084) : DOMDocument 2.6 à 6.0, loadXML, parseError, XPath,
# construction, save/load d'un fichier.
test -f "$dir/msxml_suite.exe" || { echo "msxml_suite.exe absent" >&2; exit 1; }
mkdir -p "$work/msxml"
set +e
(cd "$work/msxml" && WINLUX_GUI=headless timeout 120 "$runtime" "$dir/msxml_suite.exe") >"$work/msxml.txt" 2>&1
msxml_status=$?
set -e
cat "$work/msxml.txt"
test "$msxml_status" -eq 0 || { echo "msxml_suite.exe : code $msxml_status" >&2; exit 1; }
grep -q '^MSXML : tout est correct$' "$work/msxml.txt" || { echo "msxml_suite : marqueur de réussite absent" >&2; exit 1; }

# Annotations d'accessibilité AccPropServices (#1114) : fenêtres, menus,
# serveurs retenus, chaînes d'identité.
test -f "$dir/accprop_suite.exe" || { echo "accprop_suite.exe absent" >&2; exit 1; }
set +e
WINLUX_GUI=headless timeout 120 "$runtime" "$dir/accprop_suite.exe" >"$work/accprop.txt" 2>&1
accprop_status=$?
set -e
cat "$work/accprop.txt"
test "$accprop_status" -eq 0 || { echo "accprop_suite.exe : code $accprop_status" >&2; exit 1; }
grep -q '^AccPropServices : tout est correct$' "$work/accprop.txt" || { echo "accprop_suite : marqueur de réussite absent" >&2; exit 1; }

# WMI en lecture (#1198) : WbemLocator, ROOT\CIMV2, WQL, classes Win32_*.
test -f "$dir/wmi_suite.exe" || { echo "wmi_suite.exe absent" >&2; exit 1; }
set +e
WINLUX_GUI=headless timeout 120 "$runtime" "$dir/wmi_suite.exe" >"$work/wmi.txt" 2>&1
wmi_status=$?
set -e
cat "$work/wmi.txt"
test "$wmi_status" -eq 0 || { echo "wmi_suite.exe : code $wmi_status" >&2; exit 1; }
grep -q '^WMI : tout est correct$' "$work/wmi.txt" || { echo "wmi_suite : marqueur de réussite absent" >&2; exit 1; }

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

# Suspension de threads et contexte pour coreclr (#269), bloquant.
if test -f "$dir/threadctl_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-thread-control.sh"
fi

# Manettes et périphériques HID (#271), bloquant.
if test -f "$dir/hid_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-hid.sh"
fi

# Logiciels d'éclairage : collections de tête, HidP_*, cfgmgr32 (#1523), bloquant.
if test -f "$dir/hid_rgb_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-hid-rgb.sh"
fi

# Appareil virtuel d'un pilote constructeur (#1533), bloquant.
if test -f "$dir/vdev_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-vdev.sh"
fi

# Installation virtualisée d'un pilote constructeur (#1532), bloquant.
if test -f "$dir/driver_msvc.exe"; then
    WINLUX_MINGW_DIR="$dir" sh "$root/scripts/test-drivers.sh"
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
# Rendu Direct3D 11 depuis du HLSL source compilé par D3DCompile (#463), bloquant ;
# aussi cibles multiples et OMGetRenderTargets (#531), RSGetViewports (#1175),
# et Map WRITE d'un tampon copié depuis un tampon UAV (#1184).
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
