#!/bin/sh
# WinForms, mur 1 (#2764), sous Xvfb : le témoin bcl/temoins/FenetreBase.exe
# (Application.Run d'une Form dérivée) ouvre une vraie fenêtre de premier
# niveau titrée « Témoin WinForms », de zone cliente 320 x 200 ; fermée comme
# par l'utilisateur (WM_DELETE_WINDOW), FormClosing reçoit UserClosing,
# Application.Run rend la main et le témoin sort avec 200.
# Prérequis : scripts/build-bcl.sh --temoins et le runtime.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
exe=$root/build/bcl-temoins/FenetreBase.exe
for t in xdotool xwininfo; do
    command -v "$t" >/dev/null || { echo "$t absent : test sauté"; exit 0; }
done
test -x "$winlux" && test -f "$exe" || { echo "runtime ou témoin absent : test sauté"; exit 0; }
test -n "${DISPLAY:-}" || { echo "lancer sous xvfb-run"; exit 2; }
out=$(mktemp)
trap 'rm -f "$out"' EXIT
(cd "$root/build/bcl-temoins" && WINLUX_TEMOIN_ATTENDRE=1 WINLUX_BCL_DIR="$root/build/bcl" exec "$winlux" FenetreBase.exe) >"$out" 2>&1 &
pid=$!
n=0
win=
until win=$(xdotool search --name '^Témoin WinForms$' 2>/dev/null | head -n 1) && [ -n "$win" ]; do
    n=$((n + 1))
    if [ "$n" -gt 150 ] || ! kill -0 "$pid" 2>/dev/null; then
        code=0; wait "$pid" || code=$?
        echo "ÉCHEC : fenêtre « Témoin WinForms » jamais affichée (code $code)" >&2; cat "$out" >&2; exit 1
    fi
    sleep 0.2
done
sleep 0.5
geo=$(xwininfo -id "$win")
echo "$geo" | grep -q 'Map State: IsViewable' || { kill "$pid"; echo "ÉCHEC : fenêtre non affichée" >&2; echo "$geo" >&2; exit 1; }
# Fermeture comme par la croix de la barre de titre : message
# WM_PROTOCOLS / WM_DELETE_WINDOW (ICCCM §4.2.8), envoyé par libX11.
python3 -I - "$win" <<'PY'
import ctypes, ctypes.util, sys
x = ctypes.CDLL(ctypes.util.find_library("X11"))
x.XOpenDisplay.restype = ctypes.c_void_p
x.XInternAtom.restype = ctypes.c_ulong
x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
x.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.c_void_p]
x.XFlush.argtypes = [ctypes.c_void_p]
class ClientMessage(ctypes.Structure):
    _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int),
                ("display", ctypes.c_void_p), ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong),
                ("format", ctypes.c_int), ("l", ctypes.c_long * 5), ("pad", ctypes.c_long * 12)]
d = x.XOpenDisplay(None)
e = ClientMessage()
e.type = 33  # ClientMessage
e.window = int(sys.argv[1])
e.message_type = x.XInternAtom(d, b"WM_PROTOCOLS", 0)
e.format = 32
e.l[0] = x.XInternAtom(d, b"WM_DELETE_WINDOW", 0)
x.XSendEvent(d, e.window, 0, 0, ctypes.byref(e))
x.XFlush(d)
PY
n=0
while kill -0 "$pid" 2>/dev/null && [ "$n" -lt 50 ]; do n=$((n + 1)); sleep 0.2; done
if kill -0 "$pid" 2>/dev/null; then
    kill "$pid"; echo "ÉCHEC : Application.Run n'a pas rendu la main après la fermeture" >&2; cat "$out" >&2; exit 1
fi
code=0
wait "$pid" || code=$?
if [ "$code" != 200 ]; then
    echo "ÉCHEC : code $code au lieu de 200" >&2; cat "$out" >&2; exit 1
fi
echo "WinForms : Application.Run ouvre une vraie Form, fermée par l'utilisateur, code 200 (#2764)"
