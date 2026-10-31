#!/bin/sh
# Sessions simultanées : création, bascule, liste et fermeture, avec un faux
# serveur X (socket Unix) et un faux bureau, sans terminal virtuel réel.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-sessions-test.XXXXXX")
cleanup() {
    for p in $(cat "$work/pids" 2>/dev/null); do kill "$p" 2>/dev/null || true; done
    rm -rf "$work"
}
trap cleanup EXIT HUP INT TERM
root="$work/root"
mkdir -p "$root/etc/winlux" "$work/x11" "$work/bin"
printf 'root:root:winlux\nalice:user:secret\n' > "$root/etc/winlux/users.conf"
export WINLUX_CONTROL_ROOT="$root" WINLUX_X11_SOCKETS="$work/x11" WINLUX_TEST_LOG="$work/log"
cat > "$work/bin/Xorg" <<MOCK
#!/bin/sh
echo \$\$ >> "$work/pids"
exec python3 -c 'import socket,sys,time; s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen(1); time.sleep(600)' "$work/x11/X\${1#:}"
MOCK
cat > "$work/bin/desktop" <<MOCK
#!/bin/sh
echo \$\$ >> "$work/pids"
printf '%s %s %s\n' "\$DISPLAY" "\$WINLUX_SESSION_USER" "\$HOME" > "$work/desktop-\$WINLUX_SESSION_USER"
exec sleep 600
MOCK
chmod 755 "$work/bin/"*
export WINLUX_XORG="$work/bin/Xorg" WINLUX_SESSION_SHELL="$work/bin/desktop"
sessions() { sh "$project/scripts/winlux-sessions.sh" "$@"; }
fail() { echo "ÉCHEC : $*" >&2; cat "$root/run/winlux/sessions.txt" >&2 2>/dev/null || true; exit 1; }

# Bureau principal : root se connecte sur :0 (terminal 1).
python3 -c 'import socket,sys,time; s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen(1); time.sleep(600)' "$work/x11/X0" &
echo $! >> "$work/pids"
sleep 0.3
DISPLAY=:0 WINLUX_TEST_VT=1 sessions register root
grep -qx 'session=root|:0|1|active' "$root/run/winlux/sessions.txt" || fail "session principale absente"
if sessions switch '../x' 2>/dev/null; then fail "compte invalide accepté"; fi
if sessions switch mallory 2>/dev/null; then fail "compte inconnu accepté"; fi

# Changement rapide : alice obtient son propre écran sans fermer celui de root.
WINLUX_TEST_VT=1 sessions switch alice
grep -qx 'chvt 2' "$work/log" || fail "pas de bascule vers le terminal d'alice"
sleep 0.3
grep -q '^:1 alice /home/alice$' "$work/desktop-alice" || fail "bureau d'alice mal lancé"
test -d "$root/home/alice/Documents" || fail "dossiers personnels absents"
WINLUX_TEST_VT=2 sessions list
grep -qx 'session=alice|:1|2|active' "$root/run/winlux/sessions.txt" || fail "session alice absente"
grep -qx 'session=root|:0|1|background' "$root/run/winlux/sessions.txt" || fail "session root perdue"
test "$(sessions display alice)" = :1 || fail "écran d'alice introuvable"

# Retour vers root : simple bascule, aucune nouvelle session.
: > "$work/log"
WINLUX_TEST_VT=2 sessions switch root
grep -qx 'chvt 1' "$work/log" || fail "pas de retour vers root"
test "$(ls "$work/x11" | wc -l)" -eq 2 || fail "un serveur X de trop"

# Déconnexion d'alice depuis sa session : retour sur le bureau de root.
: > "$work/log"
WINLUX_TEST_VT=2 sessions logoff alice
grep -qx 'chvt 1' "$work/log" || fail "pas de retour après déconnexion"
sleep 0.5
WINLUX_TEST_VT=1 sessions list
if grep -q alice "$root/run/winlux/sessions.txt"; then fail "session alice encore listée"; fi
grep -qx 'session=root|:0|1|active' "$root/run/winlux/sessions.txt" || fail "session root perdue"
echo "Sessions simultanées : OK"
