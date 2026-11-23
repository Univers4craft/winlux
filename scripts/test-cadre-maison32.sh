#!/bin/sh
# Cadre maison d'un programme 32 bits (x86) : testc/wow32/cadre32.c,
# construit ici avec clang (i686-pc-windows-msvc). SetWindowPos
# (SWP_FRAMECHANGED) doit envoyer WM_NCCALCSIZE à sa procédure x86, comme à
# un programme 64 bits, sinon le cadre du gestionnaire de fenêtres reste
# par-dessus la barre de titre que le programme dessine. Avec
# WINLUX_WINDOW_DIAG, le runtime doit conclure « cadre maison true ».
# Sauté (code 0) sans runtime ni clang.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/cadre32.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for d in clang lld-link llvm-dlltool; do
    command -v "$d" >/dev/null 2>&1 || { echo "$d absent : test sauté"; exit 0; }
done
if ! test -f "$exe" || test "$root/testc/wow32/cadre32.c" -nt "$exe" || test "$0" -nt "$exe"; then
    tmp="$out/wow32-cadre"
    mkdir -p "$tmp"
    printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess@4\nGetModuleHandleW@4\n' >"$tmp/kernel32.def"
    printf 'LIBRARY user32.dll\nEXPORTS\nRegisterClassW@4\nCreateWindowExW@48\nDefWindowProcW@16\nShowWindow@8\nSetWindowPos@28\nDestroyWindow@4\n' >"$tmp/user32.def"
    for d in kernel32 user32; do
        llvm-dlltool -m i386 -k -d "$tmp/$d.def" -l "$tmp/$d.lib"
    done
    clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$root/testc/wow32/cadre32.c" -o "$tmp/cadre32.obj"
    lld-link /nologo /entry:entry /subsystem:windows /nodefaultlib /safeseh:no "$tmp/cadre32.obj" \
        "$tmp/kernel32.lib" "$tmp/user32.lib" /out:"$exe"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/home"
cd "$work"
set +e
env WINLUX_GUI=headless WINLUX_WINDOW_DIAG=1 HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
    WINLUX_REGISTRY_PATH="$work/registre.tsv" TMPDIR="$work" \
    timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
grep -a 'cadre .*WM_NCCALCSIZE' "$work/out.txt" || true
case $status in
    0) ;;
    3) echo "cadre32.exe : WM_NCCALCSIZE jamais reçu par la procédure x86 après SWP_FRAMECHANGED" >&2; exit 1 ;;
    4) echo "cadre32.exe : NCCALCSIZE_PARAMS x86 invalide (rectangle vide ou WINDOWPOS d'une autre fenêtre)" >&2; exit 1 ;;
    *) tail -20 "$work/out.txt"; echo "cadre32.exe : code $status" >&2; exit 1 ;;
esac
grep -aq 'cadre .*cadre maison true' "$work/out.txt" || {
    echo "cadre32.exe : le runtime ne conclut pas au cadre maison" >&2
    exit 1
}
echo "cadre32 : WM_NCCALCSIZE envoyé à la procédure x86, NCCALCSIZE_PARAMS x86 valide, cadre maison reconnu"
