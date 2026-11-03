#!/bin/sh
# Vérifie la langue RÉELLEMENT utilisée par Firefox (Winlux installé, compte
# de l'utilisateur, pas root) : Firefox est lancé comme pour un lien
# (winlux-browsers --launch firefox), sans fenêtre, sur un profil neuf puis
# sur le même profil relancé. Il charge une page locale, et l'en-tête
# Accept-Language qu'il envoie suit la langue de son interface (paquet de
# langue actif) : « fr-FR,fr;… » en français, « en-US,en;… » s'il est resté
# en anglais.
#
#   sh scripts/check-firefox-langue.sh          langue de Paramètres > Langue
#   sh scripts/check-firefox-langue.sh de       langue attendue imposée
#
# Code 0 si la langue est la bonne aux deux lancements, 1 sinon.
set -u
browsers=${WINLUX_BROWSERS:-winlux-browsers}
expected=${1:-}
if test -z "$expected"; then
    expected=$(sed -n 's/^LANG=//p' /etc/winlux/locale.conf 2>/dev/null | head -n 1 | cut -c1-2)
fi
test -n "$expected" || expected=fr
work=$(mktemp -d)
trap 'kill $server 2>/dev/null; rm -rf "$work"' EXIT HUP INT TERM
server=
chmod 1777 "$work"  # lancé en root, Firefox tourne sous winlux-web

python3 -I - "$work" <<'PY' &
import http.server, pathlib, socketserver, sys
work = pathlib.Path(sys.argv[1])
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/langue"):
            (work / "accept-language").write_text(self.headers.get("Accept-Language", "") + "\n")
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.end_headers()
        self.wfile.write(b"<!doctype html><title>Winlux</title>ok")
    def log_message(self, *args):
        pass
with socketserver.TCPServer(("127.0.0.1", 0), Handler) as httpd:
    (work / "port").write_text(str(httpd.server_address[1]))
    httpd.serve_forever()
PY
server=$!
for attempt in $(seq 1 50); do test -s "$work/port" && break; sleep 0.1; done
port=$(cat "$work/port" 2>/dev/null) || { echo "Serveur local indisponible" >&2; exit 1; }

failed=0
for run in 1 2; do
    rm -f "$work/accept-language"
    mkdir -p "$work/profil"
    "$browsers" --launch firefox --headless --no-remote --profile "$work/profil" "http://127.0.0.1:$port/langue?essai=$run" \
        >"$work/firefox-$run.log" 2>&1 &
    firefox=$!
    for attempt in $(seq 1 600); do test -s "$work/accept-language" && break; sleep 0.1; done
    kill "$firefox" 2>/dev/null
    pkill -f -- "--profile $work/profil" 2>/dev/null || true
    wait "$firefox" 2>/dev/null
    header=$(cat "$work/accept-language" 2>/dev/null | tr -d '\n')
    first=$(printf '%s' "$header" | cut -d, -f1 | cut -d';' -f1 | cut -c1-2 | tr 'A-Z' 'a-z')
    case "$run" in 1) what="profil neuf" ;; *) what="profil relancé" ;; esac
    if test -z "$header"; then
        echo "ÉCHEC ($what) : Firefox n'a pas chargé la page (journal : $(tail -n 3 "$work/firefox-$run.log" | tr '\n' ' '))"
        failed=1
    elif test "$first" = "$expected"; then
        echo "OK ($what) : Firefox en « $expected » (Accept-Language: $header)"
    else
        echo "ÉCHEC ($what) : Firefox en « $first » au lieu de « $expected » (Accept-Language: $header)"
        failed=1
    fi
done
if test "$failed" = 1; then
    firefox_dir=/opt/winlux/browsers/firefox/root/usr/lib/firefox
    echo "Paquets de langue installés :"
    ls "$firefox_dir/distribution/extensions" "$firefox_dir/browser/extensions" 2>&1 | sed 's/^/  /'
    echo "Paquets téléchargés :"
    ls /var/cache/winlux/browsers/ 2>&1 | grep -i l10n | sed 's/^/  /'
    grep -i 'langue' /run/winlux/browser-fetch.log 2>/dev/null | tail -n 5 | sed 's/^/  /'
fi
exit "$failed"
