#!/bin/sh
# Sous-classement comctl32 en x86 (#2876) : testc/wow32/subclass32.c,
# construit ici avec clang (i686-pc-windows-msvc), importe
# SetWindowSubclass et ses voisines par numéro d'ordre, comme Notepad++, et
# vérifie que la SUBCLASSPROC x86 reçoit ses six arguments. Sauté (code 0)
# sans runtime ni clang.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/subclass32.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for d in clang lld-link llvm-dlltool; do
    command -v "$d" >/dev/null 2>&1 || { echo "$d absent : test sauté"; exit 0; }
done
if ! test -f "$exe" || test "$root/testc/wow32/subclass32.c" -nt "$exe" || test "$0" -nt "$exe"; then
    tmp="$out/wow32-subclass"
    mkdir -p "$tmp"
    printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess@4\nGetModuleHandleW@4\n' >"$tmp/kernel32.def"
    printf 'LIBRARY user32.dll\nEXPORTS\nCreateWindowExW@48\nDestroyWindow@4\nSendMessageW@16\nGetWindowTextW@12\n' >"$tmp/user32.def"
    printf 'LIBRARY comctl32.dll\nEXPORTS\nSetWindowSubclass@16 @410 NONAME\nGetWindowSubclass@16 @411 NONAME\nRemoveWindowSubclass@12 @412 NONAME\nDefSubclassProc@16 @413 NONAME\n' >"$tmp/comctl32.def"
    for d in kernel32 user32 comctl32; do
        llvm-dlltool -m i386 -k -d "$tmp/$d.def" -l "$tmp/$d.lib"
    done
    clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$root/testc/wow32/subclass32.c" -o "$tmp/subclass32.obj"
    lld-link /nologo /entry:entry /subsystem:windows /nodefaultlib /safeseh:no "$tmp/subclass32.obj" \
        "$tmp/kernel32.lib" "$tmp/user32.lib" "$tmp/comctl32.lib" /out:"$exe"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/home"
cd "$work"
set +e
env WINLUX_GUI=headless HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
    WINLUX_REGISTRY_PATH="$work/registre.tsv" TMPDIR="$work" \
    timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    tail -20 "$work/out.txt"
    echo "subclass32.exe : code $status (vérification en échec, ou plantage du code x86)" >&2
    exit 1
}
echo "subclass32 : SUBCLASSPROC x86 importée par numéro d'ordre, six arguments, DefSubclassProc, retrait"
