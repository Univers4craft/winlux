#!/bin/sh
# Icône de fenêtre (#2836), sous Xvfb : le témoin bcl/temoins/Icones.exe pose
# Form.Icon (WM_SETICON, petite et grande) ; la fenêtre X11 doit alors porter
# cette icône dans _NET_WM_ICON (EWMH) : une image 32 x 32 bleue
# (0xFF000064), à la place du logo Winlux posé à la création (16, 32, 48).
# Prérequis : scripts/build-bcl.sh --temoins et le runtime.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
exe=$root/build/bcl-temoins/Icones.exe
for t in xdotool xprop; do
    command -v "$t" >/dev/null || { echo "$t absent : test sauté"; exit 0; }
done
test -x "$winlux" && test -f "$exe" || { echo "runtime ou témoin absent : test sauté"; exit 0; }
test -n "${DISPLAY:-}" || { echo "lancer sous xvfb-run"; exit 2; }
out=$(mktemp)
trap 'rm -f "$out"' EXIT
# Hors session Winlux, la fenêtre reçoit d'abord le logo (#1425) : le test
# vérifie que l'icône de l'appli le remplace.
(cd "$root/build/bcl-temoins" && XDG_SESSION_DESKTOP=autre WINLUX_TEMOIN_ATTENDRE=1 \
    WINLUX_BCL_DIR="$root/build/bcl" exec "$winlux" Icones.exe) >"$out" 2>&1 &
pid=$!
n=0
win=
until win=$(xdotool search --name '^Témoin icône$' 2>/dev/null | head -n 1) && [ -n "$win" ]; do
    n=$((n + 1))
    if [ "$n" -gt 150 ] || ! kill -0 "$pid" 2>/dev/null; then
        code=0; wait "$pid" || code=$?
        echo "ÉCHEC : fenêtre « Témoin icône » jamais affichée (code $code)" >&2; cat "$out" >&2; exit 1
    fi
    sleep 0.2
done
sleep 0.5
icone=$(xprop -id "$win" 32c _NET_WM_ICON 2>&1 || true)
python3 -I - "$icone" <<'PY' || { kill "$pid" 2>/dev/null; cat "$out" >&2; exit 1; }
import sys
texte = sys.argv[1]
if "=" not in texte:
    sys.exit("ÉCHEC : pas de _NET_WM_ICON : " + texte[:200])
v = [int(x) for x in texte.split("=", 1)[1].replace(",", " ").split()]
tailles, i = [], 0
while i + 2 <= len(v):
    w, h = v[i], v[i + 1]
    tailles.append((w, h, v[i + 2:i + 2 + w * h]))
    i += 2 + w * h
if [(w, h) for w, h, _ in tailles] != [(32, 32)]:
    sys.exit("ÉCHEC : _NET_WM_ICON porte %s, une seule image 32 x 32 attendue" % [(w, h) for w, h, _ in tailles])
if set(tailles[0][2]) != {0xFF000064}:
    sys.exit("ÉCHEC : pixels %s, 0xff000064 attendu" % sorted({hex(p) for p in tailles[0][2]})[:4])
PY
# Fermeture comme par la croix (WM_DELETE_WINDOW), le témoin finit ses
# vérifications et sort avec 200.
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
e.type = 33
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
    kill "$pid"; echo "ÉCHEC : le témoin n'a pas rendu la main après la fermeture" >&2; cat "$out" >&2; exit 1
fi
code=0
wait "$pid" || code=$?
if [ "$code" != 200 ]; then
    echo "ÉCHEC : code $code au lieu de 200" >&2; cat "$out" >&2; exit 1
fi
echo "Fenêtres : l'icône posée par WM_SETICON est publiée dans _NET_WM_ICON (#2836)"
