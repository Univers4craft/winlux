#!/bin/sh
# System.Net (#2814) : le témoin bcl/temoins/Reseau.cs contre un vrai serveur
# HTTP local. HttpWebRequest passe par notre WinHTTP : GET entier, reprise
# AddRange (206), 404 en WebException ProtocolError, WebProxy direct et
# mandataire nommé, refus de connexion. Le serveur ne sert que 127.0.0.1.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
winlux=${WINLUX_BIN:-$root/target/release/winlux}
temoins=$root/build/bcl-temoins
test -x "$winlux" || { echo "runtime absent : test sauté"; exit 0; }
test -f "$temoins/Reseau.exe" || { echo "lancer scripts/build-bcl.sh --temoins d'abord : test sauté"; exit 0; }
command -v python3 >/dev/null || { echo "python3 absent : test sauté"; exit 0; }
command -v curl >/dev/null || { echo "curl absent (WinHTTP s'en sert) : test sauté"; exit 0; }

# Aucun mandataire du système : seul celui que le témoin nomme compte.
unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY all_proxy ALL_PROXY no_proxy NO_PROXY

work=$(mktemp -d)
pid=
trap '[ -n "$pid" ] && kill "$pid" 2>/dev/null; rm -rf "$work"' EXIT
cat >"$work/serveur.py" <<'PY'
import http.server, sys, urllib.parse

DONNEES = bytes(i & 0xFF for i in range(4096))

class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def do_GET(self):
        chemin, relais = self.path, False
        if chemin.startswith("http://"):  # URL absolue : on fait le mandataire
            chemin, relais = urllib.parse.urlsplit(chemin).path, True
        if chemin != "/donnees":
            self.send_response(404)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        corps, code = DONNEES, 200
        plage = self.headers.get("Range")
        debut = 0
        if plage and plage.startswith("bytes="):
            de, _, a = plage[6:].partition("-")
            debut = int(de)
            fin = int(a) if a else len(DONNEES) - 1
            corps, code = DONNEES[debut:fin + 1], 206
        self.send_response(code)
        if code == 206:
            self.send_header("Content-Range", "bytes %d-%d/%d" % (debut, debut + len(corps) - 1, len(DONNEES)))
        self.send_header("Content-Length", str(len(corps)))
        self.send_header("X-Agent", self.headers.get("User-Agent", ""))
        if relais:
            self.send_header("X-Mandataire", "oui")
        self.end_headers()
        self.wfile.write(corps)

s = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
print(s.server_address[1], flush=True)
s.serve_forever()
PY
python3 -I "$work/serveur.py" >"$work/port" &
pid=$!
i=0
while [ ! -s "$work/port" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
port=$(cat "$work/port")
test -n "$port" || { echo "ÉCHEC : le serveur local n'a pas démarré"; exit 1; }

set +e
(cd "$temoins" && WINLUX_BCL_DIR="$root/build/bcl" WINLUX_TEMOIN_URL="http://127.0.0.1:$port" \
    timeout 120 "$winlux" Reseau.exe)
code=$?
set -e
if [ "$code" != 200 ]; then
    echo "ÉCHEC : témoin Reseau, code $code (200 attendu ; 3 à 9 = vérification réseau)"
    exit 1
fi
echo "ok : HttpWebRequest sur WinHTTP contre 127.0.0.1:$port (#2814)"
