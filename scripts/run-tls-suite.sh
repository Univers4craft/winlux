#!/bin/sh
# Lance tls_suite.exe contre un serveur HTTPS local (openssl s_server -www,
# certificat auto-signé jetable). Sans la commande openssl, la suite tourne
# quand même : seule la poignée de main Schannel est sautée.
# Usage : run-tls-suite.sh <runtime winlux> <tls_suite.exe>
set -u
runtime=$1
exe=$2
if ! command -v openssl >/dev/null 2>&1; then
    WINLUX_GUI=headless "$runtime" "$exe"
    exit $?
fi
dir=$(mktemp -d)
pid=
cleanup() {
    [ -n "$pid" ] && kill "$pid" 2>/dev/null
    rm -rf "$dir"
}
trap cleanup EXIT
openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=localhost -days 1 \
    -keyout "$dir/key.pem" -out "$dir/cert.pem" >/dev/null 2>&1
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
openssl s_server -quiet -www -accept "$port" -cert "$dir/cert.pem" -key "$dir/key.pem" >/dev/null 2>&1 &
pid=$!
i=0
while [ $i -lt 50 ] && ! python3 -c "import socket; socket.create_connection(('127.0.0.1',$port),1)" 2>/dev/null; do
    sleep 0.1
    i=$((i + 1))
done
WINLUX_TLS_PORT=$port WINLUX_GUI=headless "$runtime" "$exe"
