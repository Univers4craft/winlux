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
# wow64.exe (étape 7) : personnalité WOW64 (IsWow64Process, registre
# Wow6432Node, System32 → SysWOW64, variables) et structures kernel32 à la
# taille x86 (CRITICAL_SECTION, SRWLOCK, SLIST, SYSTEM_INFO, STARTUPINFO…).
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
stale=0
names="hello32 seh32 wow64"
command -v i686-w64-mingw32-gcc >/dev/null 2>&1 && names="$names crt32 thread32 load32"
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
echo "hello32 : programme x86 lancé, 15 vérifications passées"
run hello32_reloc
echo "hello32_reloc : image déplacée sous 2 Go, relocations HIGHLOW"
run hello32 WINLUX_WOW32_NO_FSGSBASE=1
echo "hello32 : FS rétabli par arch_prctl (sans FSGSBASE)"

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
echo "seh32 : 9 cas __try/__except/__finally (NULL, division, RaiseException, déroulement, ud2, reprise)"
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
    echo "crt32 : CRT msvcrt 32 bits, 20 vérifications (printf de MinGW et de msvcrt)"
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
if test -f "$out/thread32.exe"; then
    check thread32 'Threads 32 bits : tout est correct'
    check thread32_gcc 'Threads 32 bits : tout est correct'
    echo "thread32 : threads x86, TLS statique et émulé, 300 threads, rappel, GetProcAddress"
    check load32 'DLL 32 bits : tout est correct'
    echo "load32 : DLL x86 importée (DllMain, TLS, donnée) et chargée par LoadLibrary"
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
