#!/bin/sh
# Fenêtre d'un programme 32 bits (x86) agrandie par le gestionnaire de
# fenêtres (testc/wow32/agrandie32.c, construit ici avec clang
# i686-pc-windows-msvc). Sous openbox réel, wmctrl agrandit la fenêtre ; sa
# procédure x86 doit recevoir WM_WINDOWPOSCHANGED, avec un WINDOWPOS x86 à
# la nouvelle taille, avant WM_SIZE, comme un programme 64 bits (#1476) :
# Chromium et CEF ne redimensionnent leur rendu que sur ce message.
# Sauté (code 0) sans runtime, clang, Xvfb, openbox ou wmctrl.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/agrandie32.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for d in clang lld-link llvm-dlltool Xvfb openbox wmctrl xprop xdotool; do
    command -v "$d" >/dev/null 2>&1 || { echo "$d absent : test sauté"; exit 0; }
done
if ! test -f "$exe" || test "$root/testc/wow32/agrandie32.c" -nt "$exe" || test "$0" -nt "$exe"; then
    tmp="$out/wow32-agrandie"
    mkdir -p "$tmp"
    printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess@4\nGetModuleHandleW@4\n' >"$tmp/kernel32.def"
    printf 'LIBRARY user32.dll\nEXPORTS\nRegisterClassW@4\nCreateWindowExW@48\nDefWindowProcW@16\nShowWindow@8\nGetMessageW@16\nDispatchMessageW@4\nSetTimer@16\n' >"$tmp/user32.def"
    for d in kernel32 user32; do
        llvm-dlltool -m i386 -k -d "$tmp/$d.def" -l "$tmp/$d.lib"
    done
    clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$root/testc/wow32/agrandie32.c" -o "$tmp/agrandie32.obj"
    lld-link /nologo /entry:entry /subsystem:windows /nodefaultlib /safeseh:no "$tmp/agrandie32.obj" \
        "$tmp/kernel32.lib" "$tmp/user32.lib" /out:"$exe"
fi

work=$(mktemp -d)
pids=
cleanup() { kill $pids 2>/dev/null || true; rm -rf "$work"; }
trap cleanup EXIT
mkdir -p "$work/home"
Xvfb -displayfd 3 -screen 0 1280x1024x24 -nolisten tcp 3>"$work/display" >/dev/null 2>&1 & pids="$pids $!"
for _ in $(seq 100); do test -s "$work/display" && break; sleep 0.1; done
export DISPLAY=:$(head -1 "$work/display")
for _ in $(seq 50); do xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.1; done
HOME=$work/home openbox --sm-disable >/dev/null 2>&1 & pids="$pids $!"
for _ in $(seq 300); do xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null | grep -q "window id" && break; sleep 0.1; done

(cd "$work" && exec env HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
    WINLUX_REGISTRY_PATH="$work/registre.tsv" TMPDIR="$work" \
    timeout 60 "$runtime" "$exe") >"$work/out.txt" 2>&1 &
prog=$!
# Fenêtre gérée par openbox ; un événement X réveille sa boucle si la
# MapRequest attend dans sa file (#3190, openbox d'Ubuntu).
gere=
for _ in $(seq 150); do
    if wmctrl -l 2>/dev/null | grep -q 'Agrandie32'; then gere=1; break; fi
    xprop -root -f _WINLUX_REVEIL 32c -set _WINLUX_REVEIL 1 2>/dev/null || true
    sleep 0.2
done
if [ -z "$gere" ]; then
    kill "$prog" 2>/dev/null || true
    cat "$work/out.txt" >&2
    echo "ÉCHEC : fenêtre Agrandie32 jamais gérée par openbox" >&2
    exit 1
fi
wmctrl -r Agrandie32 -b add,maximized_vert,maximized_horz
set +e
wait "$prog"
status=$?
set -e
case $status in
    0) echo "agrandie32 : WM_WINDOWPOSCHANGED reçu par la procédure x86 avant WM_SIZE, WINDOWPOS x86 valide (#1476)" ;;
    5) echo "ÉCHEC : agrandie par le gestionnaire, la procédure x86 reçoit WM_SIZE sans WM_WINDOWPOSCHANGED" >&2; exit 1 ;;
    6) echo "ÉCHEC : WM_WINDOWPOSCHANGED avec un WINDOWPOS x86 invalide" >&2; exit 1 ;;
    7) echo "ÉCHEC : la fenêtre n'a jamais reçu de WM_SIZE agrandi (20 s)" >&2; exit 1 ;;
    *) tail -20 "$work/out.txt" >&2; echo "ÉCHEC : agrandie32.exe code $status" >&2; exit 1 ;;
esac
