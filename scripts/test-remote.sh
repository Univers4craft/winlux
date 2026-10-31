#!/bin/sh
# Bureau à distance : réglages, démarrage du serveur winlux-remote, offre de
# sécurité RFB (chiffrement obligatoire par défaut) et arrêt, dans une
# racine temporaire. Le chiffrement lui-même est couvert par cargo test.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
bin=${WINLUX_REMOTE_BIN:-$project/target/release/winlux-remote}
test -x "$bin" || { echo "winlux-remote absent : cargo build --release -p winlux" >&2; exit 1; }
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-remote-test.XXXXXX")
root="$work/root"
export WINLUX_CONTROL_ROOT="$root" WINLUX_REMOTE_BIN="$bin" WINLUX_REMOTE_KEY_BITS=1024
config() { sh "$project/scripts/winlux-remote-config.sh" "$@"; }
trap 'config disable >/dev/null 2>&1 || true; rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$root/etc/winlux"
printf 'root:root:admin\nalice:user:s3cret\nbob:user:nopass\n' > "$root/etc/winlux/users.conf"
state="$root/run/winlux/remote.txt"
fail() { echo "ÉCHEC : $*" >&2; cat "$state" >&2 2>/dev/null || true; cat "$root/run/winlux/remote.log" >&2 2>/dev/null || true; exit 1; }
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')

config status
grep -qx 'enabled=0' "$state" || fail "activé par défaut"
grep -qx 'allowed=root' "$state" || fail "les administrateurs doivent être autorisés par défaut"
grep -qx 'account=bob|user|0' "$state" || fail "comptes non listés"
if config port 80 2>/dev/null; then fail "port réservé accepté"; fi
if config allow mallory 2>/dev/null; then fail "compte inconnu accepté"; fi
config port "$port"
config allow alice
grep -qx 'allowed=root,alice' "$state" || fail "alice non autorisée"
config deny root
grep -qx 'allowed=alice' "$state" || fail "root non retiré"

config enable
grep -qx 'running=1' "$state" || fail "serveur non démarré"
grep -q '^fingerprint=[0-9a-f][0-9a-f]-' "$state" || fail "empreinte absente"
test -f "$root/etc/winlux/remote-key" || fail "clé RSA non enregistrée"
test "$(stat -c %a "$root/etc/winlux/remote-key")" = 600 || fail "clé RSA lisible par tous"

# Offre de sécurité : RSA-AES seulement, puis aussi le défi VNC si activé.
offer() {
    python3 - "$port" <<'PY'
import socket, sys
s = socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=10)
assert s.recv(12) == b"RFB 003.008\n"
s.sendall(b"RFB 003.008\n")
n = s.recv(1)[0]
types = s.recv(n)
print(",".join(str(t) for t in types))
PY
}
test "$(offer)" = "129,5" || fail "offre de sécurité inattendue : $(offer)"
config legacy-on
pw=$(sed -n 's/^legacy_password=//p' "$state")
test "${#pw}" -eq 8 || fail "mot de passe VNC absent"
test "$(offer)" = "129,5,2" || fail "défi VNC non proposé"
config legacy-off
test ! -e "$root/etc/winlux/remote-vnc.passwd" || fail "mot de passe VNC conservé"
test "$(offer)" = "129,5" || fail "défi VNC encore proposé"

# Changement de port : le serveur redémarre sur le nouveau port.
config port-up
port=$((port + 1))
grep -qx "port=$port" "$state" || fail "port non changé"
sleep 0.5
test "$(offer)" = "129,5" || fail "serveur absent sur le nouveau port"

config disable
grep -qx 'running=0' "$state" || fail "serveur non arrêté"
echo "Bureau à distance : OK"
