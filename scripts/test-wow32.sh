#!/bin/sh
# Programmes Windows 32 bits (x86) sous Winlux (#232) : construit
# testc/wow32/hello32.exe avec clang (cible i686-pc-windows-msvc) si besoin,
# puis vérifie qu'il démarre et que ses quinze vérifications passent :
# TEB 32 bits sur FS, PEB, pile, dernière erreur, mémoire sous 2 Go,
# argument et retour de 64 bits, pseudo-handle -1, ligne de commande,
# environnement, nettoyage de pile stdcall. Deux fois de plus : image
# déplacée (relocations HIGHLOW) et base de FS rétablie par arch_prctl,
# comme sur un processeur sans FSGSBASE. Enfin, un accès à NULL dans le
# code x86 doit finir au rapport de plantage (code 0xC0000005, octet bas 5).
# seh32.exe (étape 2) : blocs __try/__except/__finally de clang, fautes
# matérielles, RaiseException, exception non rattrapée et filtre de dernier
# recours.
# crt32.exe et crt32_msvcrt.exe (étape 3) : vrais programmes MinGW i686 liés
# à msvcrt.dll (argv, printf/sscanf, qsort, setjmp, fichiers, atexit,
# signal, localtime…), sautés sans i686-w64-mingw32-gcc.
# thread32.exe, load32.exe, cpp32.exe (étape 4) : threads x86, TLS statique,
# DLL x86 importée et chargée par LoadLibrary, C++ avec exceptions.
# com32.exe et dlg32.exe (étape 6) : COM (IStream, fabrique x86, VARIANT,
# IErrorInfo), contrôles communs et boîtes Ouvrir/Couleur/Police/Imprimer.
# wow64.exe (étape 7) : personnalité WOW64 (IsWow64Process, registre
# Wow6432Node, System32 → SysWOW64, variables) et structures kernel32 à la
# taille x86 (CRITICAL_SECTION, SRWLOCK, SLIST, SYSTEM_INFO, STARTUPINFO…).
# cxx32.exe (étape 7) : exceptions C++ MSVC x86 (__CxxFrameHandler3).
# gui32.exe (étape 5) : fenêtre x86 (classe, CREATESTRUCT, boucle de
# messages, WM_PAINT et GDI, clavier, souris, minuteurs, sous-classement,
# WINDOWPOS recopiée, DIB section, GetObject, boîte de dialogue), sans écran
# puis sous un vrai serveur X (Xvfb) quand il est là.
# jeux32.exe et jeux64.exe (#237, #777) : Direct3D 8, DirectDraw (surfaces 8/16/32 bits,
# Lock, Blt, BltFast, clés de couleur, GetDC, palette, découpeur, plein écran
# exclusif et Flip), DirectPlay 4 et 8 (deux joueurs sur 127.0.0.1), MIDI (midiOut*, midiStream* et leurs rappels),
# timeSetEvent et waveOut avec rappel, en x86 puis en x64.
# wasapi32.exe et wasapi64.exe : WASAPI en x86 et en x64 (IMMDeviceEnumerator,
# PROPVARIANT chaîne et BLOB, IAudioClient, tampon de rendu, volumes flottants).
# d3d1132.exe et d3d1164.exe (#1090) : Direct3D 11 et DXGI en x86 et en x64,
# sous Xvfb (contexte OpenGL).
# d3d1032.exe et d3d1064.exe (#1090, étape 2) : Direct3D 10 et 10.1 sur
# Direct3D 11, en x86 et en x64, sous Xvfb.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
stale=0
names="hello32 seh32 wow64 cxx32 gui32 jeux32 d3d1132 d3d1032 wasapi32"
command -v i686-w64-mingw32-gcc >/dev/null 2>&1 && names="$names crt32 net32 thread32 load32 fiber32 com32 dlg32"
for name in $names; do
    if ! test -f "$out/$name.exe" || test "$root/testc/wow32/$name.c" -nt "$out/$name.exe" \
        || test "$root/testc/wow32/build.sh" -nt "$out/$name.exe"; then
        stale=1
    fi
done
if test "$stale" = 1; then
    sh "$root/testc/wow32/build.sh" "$out"
fi
test -f "$out/hello32.exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

run() {
    name=$1; shift
    set +e
    env WINLUX_GUI=headless WINLUX_WOW32_TEST=ok "$@" timeout 60 "$runtime" "$out/$name.exe" argument-de-test >"$work/out.txt" 2>&1
    status=$?
    set -e
    if test "$status" -ne 0 || ! grep -q 'Tout est correct' "$work/out.txt"; then
        cat "$work/out.txt"
        echo "$name.exe ($*) : code $status, vérification n° $status en échec" >&2
        exit 1
    fi
    grep -q 'Bonjour depuis un programme Windows 32 bits' "$work/out.txt"
}

run hello32
echo "hello32 : programme x86 lancé, 25 vérifications passées (dont Vulkan sans pilote 32 bits, #1951)"
run hello32_reloc
echo "hello32_reloc : image déplacée sous 2 Go, relocations HIGHLOW"
run hello32 WINLUX_WOW32_NO_FSGSBASE=1
echo "hello32 : FS rétabli par arch_prctl (sans FSGSBASE)"
run hello32 WINLUX_WOW32_STEP=GetEnvironmentVariableA
echo "hello32 : pas à pas (WINLUX_WOW32_STEP) sans perdre FS"

set +e
WINLUX_GUI=headless WINLUX_CRASH_DIR="$work/crash" timeout 60 "$runtime" "$out/hello32.exe" plantage >"$work/crash.txt" 2>&1
crash=$?
set -e
if test "$crash" -ne 5 || ! grep -q EXCEPTION_ACCESS_VIOLATION "$work/crash.txt"; then
    cat "$work/crash.txt"
    echo "hello32.exe plantage : code $crash, 5 et rapport de plantage attendus" >&2
    exit 1
fi
echo "hello32 : faute dans le code x86 -> rapport de plantage"

expect() {
    want=$1 text=$2; shift 2
    set +e
    WINLUX_GUI=headless timeout 60 "$runtime" "$out/seh32.exe" "$@" >"$work/seh.txt" 2>&1
    got=$?
    set -e
    if test "$got" -ne "$want" || ! grep -q "$text" "$work/seh.txt"; then
        cat "$work/seh.txt"
        echo "seh32.exe $* : code $got, $want attendu (« $text »)" >&2
        exit 1
    fi
}
expect 0 "SEH 32 bits : tout est correct"
echo "seh32 : 11 cas __try/__except/__finally (NULL, division, RaiseException, déroulement, ud2, reprise, rappel 64 bits rattrapé par saut direct)"
WINLUX_WOW32_NO_FSGSBASE=1 expect 0 "SEH 32 bits : tout est correct"
echo "seh32 : idem sans FSGSBASE"
expect 66 "0xE0000042 non gérée" nonrattrapee
echo "seh32 : exception non rattrapée -> message et code 0xE0000042"
expect 77 "filtre de dernier recours appele" filtre
echo "seh32 : filtre SetUnhandledExceptionFilter appelé"

if test -f "$out/crt32.exe"; then
    for name in crt32 crt32_msvcrt; do
        set +e
        WINLUX_GUI=headless WINLUX_WOW32_TEST=ok timeout 60 "$runtime" "$out/$name.exe" argument-de-test >"$work/crt.txt" 2>&1
        status=$?
        set -e
        if test "$status" -ne 0 || ! grep -q 'CRT 32 bits : tout est correct' "$work/crt.txt" \
            || ! grep -q 'atexit appele' "$work/crt.txt" || ! grep -q 'sur stderr 3' "$work/crt.txt"; then
            cat "$work/crt.txt"
            echo "$name.exe : code $status, vérification n° $status en échec" >&2
            exit 1
        fi
    done
    echo "crt32 : CRT msvcrt 32 bits, 44 vérifications (printf de MinGW et de msvcrt)"
else
    echo "crt32 : i686-w64-mingw32-gcc absent, test sauté"
fi

check() {
    name=$1 text=$2
    set +e
    WINLUX_GUI=headless timeout 120 "$runtime" "$out/$name.exe" >"$work/$name.txt" 2>&1
    status=$?
    set -e
    if test "$status" -ne 0 || ! grep -q "$text" "$work/$name.txt"; then
        cat "$work/$name.txt"
        echo "$name.exe : code $status, vérification n° $status en échec" >&2
        exit 1
    fi
}
if test -f "$out/net32.exe"; then
    check net32 'Réseau 32 bits : tout est correct'
    echo "net32 : Winsock x86 (AcceptEx, ConnectEx, GetAcceptExSockaddrs, OVERLAPPED x86 et port de complétion)"
fi
if test -f "$out/thread32.exe"; then
    check thread32 'Threads 32 bits : tout est correct'
    check thread32_gcc 'Threads 32 bits : tout est correct'
    echo "thread32 : threads x86, TLS statique et émulé, 300 threads, rappel, GetProcAddress"
    set +e
    WINLUX_GETPROC_STUBS=1 WINLUX_GUI=headless timeout 120 "$runtime" "$out/thread32.exe" >"$work/stubs.txt" 2>&1
    status=$?
    set -e
    if test "$status" -ne 0 || ! grep -q 'Threads 32 bits : tout est correct' "$work/stubs.txt" \
        || ! grep -q 'kernel32!WinluxFonctionAbsente absente : bouchon rendu' "$work/stubs.txt"; then
        cat "$work/stubs.txt"
        echo "thread32.exe (WINLUX_GETPROC_STUBS=1) : code $status" >&2
        exit 1
    fi
    echo "thread32 : WINLUX_GETPROC_STUBS=1 rend un bouchon journalisé pour une fonction absente"
    check load32 'DLL 32 bits : tout est correct'
    echo "load32 : DLL x86 importée (DllMain, TLS, donnée) et chargée par LoadLibrary"
    check fiber32 'Fibres 32 bits : tout est correct'
    echo "fiber32 : fibres x86 (SwitchToFiber aller-retour, pile propre, fin de routine = fin du thread)"
else
    echo "thread32 : i686-w64-mingw32-gcc absent, test sauté"
fi
if test -f "$out/cpp32.exe"; then
    check cpp32 'C++ 32 bits : tout est correct'
    echo "cpp32 : C++ MinGW 32 bits (exceptions, iostream)"
fi

# Personnalité WOW64 : lecteur C: et registre isolés dans le dossier de test.
mkdir -p "$work/c/Windows/System32" "$work/c/Windows/SysWOW64"
set +e
WINLUX_GUI=headless WINLUX_DRIVE_C="$work/c" WINLUX_REGISTRY_PATH="$work/wow64-reg.tsv" \
    timeout 120 "$runtime" "$out/wow64.exe" >"$work/wow64.txt" 2>&1
status=$?
set -e
if test "$status" -ne 0 || ! grep -q 'Tout est correct : personnalite WOW64' "$work/wow64.txt"; then
    cat "$work/wow64.txt"
    echo "wow64.exe : code $status, vérification n° $status en échec" >&2
    exit 1
fi
for f in SysWOW64/winlux-wow64-redirige.txt System32/winlux-wow64-direct.txt \
    System32/winlux-wow64-sysnative.txt; do
    test -f "$work/c/Windows/$f" || { echo "wow64.exe : $f absent (redirection System32)" >&2; exit 1; }
done
if test -e "$work/c/Windows/System32/winlux-wow64-redirige.txt"; then
    echo "wow64.exe : fichier écrit dans System32 malgré la redirection" >&2
    exit 1
fi
echo "wow64 : IsWow64Process, structures x86 (CS, SRW, SLIST, SYSTEM_INFO…), Wow6432Node, SysWOW64, CreateProcess"

check cxx32 'C++ MSVC 32 bits : tout est correct'
echo "cxx32 : exceptions C++ MSVC x86 (catch par valeur et référence, catch(...), throw;, try dans un catch)"
if test -f "$out/com32.exe"; then
    check com32 'COM 32 bits, .* Tout est correct'
    echo "com32 : COM x86 (IStream du runtime, CopyTo vers un flux x86, fabrique x86, VARIANT, IErrorInfo, NetworkListManager)"
    check dlg32 'boîtes communes 32 bits, .* Tout est correct'
    echo "dlg32 : liste, arbre, barre d'outils, onglets, Ouvrir, Enregistrer, Couleur, Police, Imprimer en x86"
    check msxml32 'MSXML : tout est correct'
    echo "msxml32 : DOMDocument MSXML en x86 (VARIANT par valeur, nœuds, XPath, save/load)"
    check accprop32 'AccPropServices : tout est correct'
    echo "accprop32 : annotations d'accessibilité en x86 (MSAAPROPID et VARIANT par valeur, IAccPropServer)"
    check wmi32 'WMI : tout est correct'
    echo "wmi32 : WMI en x86 (WbemLocator, ExecQuery, VARIANT, CIMTYPE, SAFEARRAY de noms)"
fi

gui() {
    set +e
    "$@" timeout 60 "$runtime" "$out/gui32.exe" >"$work/gui.txt" 2>&1
    status=$?
    set -e
    if test "$status" -ne 0 || ! grep -q 'GUI 32 bits : tout est correct' "$work/gui.txt"; then
        cat "$work/gui.txt"
        echo "gui32.exe ($*) : code $status, vérification n° $status en échec" >&2
        exit 1
    fi
}
gui env WINLUX_GUI=headless
echo "gui32 : fenêtre x86 sans écran (WNDPROC, MSG, PAINTSTRUCT, GDI, minuteurs, dialogue)"
gui env WINLUX_GUI=headless WINLUX_WOW32_NO_FSGSBASE=1
echo "gui32 : idem sans FSGSBASE"
if command -v xvfb-run >/dev/null 2>&1; then
    gui xvfb-run -a -s '-screen 0 1024x768x24' env -u WINLUX_GUI
    echo "gui32 : fenêtre x86 sous X11 (Xvfb)"
else
    echo "gui32 : xvfb-run absent, essai X11 sauté"
fi

jeux() {
    name=$1; shift
    set +e
    "$@" WINLUX_DDRAW_MODESWITCH=0 WINLUX_MIDI_IN_DIR="$work/midi-in" timeout 60 "$runtime" "$out/$name.exe" >"$work/jeux.txt" 2>&1
    status=$?
    set -e
    if test "$status" -ne 0 || ! grep -q 'Jeux anciens : tout est correct' "$work/jeux.txt"; then
        cat "$work/jeux.txt"
        echo "$name.exe ($*) : code $status, vérification n° $status en échec" >&2
        exit 1
    fi
}
# Entrée MIDI sans clavier : un fichier à la place de /dev/snd/midiC0D0.
mkdir -p "$work/midi-in"
printf '\220\074\177\074\000\370\360\176\177\011\001\367\300\005' >"$work/midi-in/midiC0D0"
for name in jeux32 jeux64; do
    jeux "$name" env WINLUX_GUI=headless
    if command -v xvfb-run >/dev/null 2>&1; then
        jeux "$name" xvfb-run -a -s '-screen 0 1024x768x24' env -u WINLUX_GUI
    fi
done
echo "jeux32/jeux64 : DirectDraw, Direct3D 8, Direct3D 9 et D3DX9, DirectSound, DirectShow, DirectPlay 4 et 8, MIDI (synthétiseur, flux, entrée), msacm32 (conversions PCM, décodeurs ADPCM), timeSetEvent et waveOut avec rappels, x86 et x64"

# WASAPI : un programme x86 recevait E_NOINTERFACE sur IMMDeviceEnumerator.
for name in wasapi32 wasapi64; do
    set +e
    WINLUX_AUDIO=null WINLUX_GUI=headless timeout 60 "$runtime" "$out/$name.exe" >"$work/wasapi.txt" 2>&1
    status=$?
    set -e
    if test "$status" -ne 0 || ! grep -q 'WASAPI : tout est correct' "$work/wasapi.txt" \
        || grep -q 'non décrite pour les programmes 32 bits' "$work/wasapi.txt"; then
        tail -20 "$work/wasapi.txt"
        echo "$name.exe : code $status, vérification n° $status en échec" >&2
        exit 1
    fi
done
echo "wasapi32/wasapi64 : WASAPI en x86 et en x64 (énumération, PROPVARIANT chaîne et BLOB, IAudioClient, rendu, horloge, volumes flottants)"

# Direct3D 11 et DXGI (#1090) : il faut un contexte OpenGL, donc Xvfb.
if command -v xvfb-run >/dev/null 2>&1; then
    for name in d3d1132 d3d1164; do
        set +e
        xvfb-run -a -s '-screen 0 1024x768x24' env -u WINLUX_GUI timeout 60 "$runtime" "$out/$name.exe" >"$work/d3d11.txt" 2>&1
        status=$?
        set -e
        if test "$status" -ne 0 || ! grep -q 'Direct3D 11 : tout est correct' "$work/d3d11.txt"; then
            tail -20 "$work/d3d11.txt"
            echo "$name.exe : code $status, vérification n° $status en échec" >&2
            exit 1
        fi
    done
    echo "d3d1132/d3d1164 : Direct3D 11 et DXGI en x86 et en x64 (données initiales, Map, tableaux de vues, DXGI_SWAP_CHAIN_DESC, DXGI_ADAPTER_DESC)"
    for name in d3d1032 d3d1064; do
        set +e
        xvfb-run -a -s '-screen 0 1024x768x24' env -u WINLUX_GUI timeout 60 "$runtime" "$out/$name.exe" >"$work/d3d10.txt" 2>&1
        status=$?
        set -e
        if test "$status" -ne 0 || ! grep -q 'Direct3D 10 : tout est correct' "$work/d3d10.txt"; then
            tail -20 "$work/d3d10.txt"
            echo "$name.exe : code $status, vérification n° $status en échec" >&2
            exit 1
        fi
    done
    echo "d3d1032/d3d1064 : Direct3D 10 et 10.1 sur Direct3D 11 en x86 et en x64 (D3D10_MAPPED_TEXTURE2D, D3D10_VIEWPORT, D3D10_BLEND_DESC, tableaux d'objets, chaîne d'échange)"
else
    echo "d3d1132 : xvfb-run absent, test Direct3D 11 sauté"
fi
