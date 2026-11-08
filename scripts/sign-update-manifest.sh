#!/bin/sh
# Signe le manifeste de mise à jour (#31) : MANIFESTE.sig à côté de lui.
#
# Usage : sh scripts/sign-update-manifest.sh [MANIFESTE] [--obligatoire]
#
# La graine Ed25519 (32 octets en hexadécimal) vient de la variable
# WINLUX_UPDATE_SIGNING_KEY (secret GitHub du même nom) ; elle n'est écrite
# que dans un fichier temporaire 0600, effacé à la sortie. La signature est
# revérifiée avec la clé publique du dépôt (integration/native/keys/
# update-signing.pub), celle que les PC installés embarquent.
#
# Sans graine : avertissement et sortie 0, sauf avec --obligatoire ou si la
# clé publique est déjà dans le dépôt (les PC refuseraient un manifeste non
# signé) : sortie 1.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
manifest=${1:-build/winlux-update-manifest.txt}
required=0
test "${2:-}" = "--obligatoire" && required=1
public="$root/integration/native/keys/update-signing.pub"
test -f "$public" && required=1
test -f "$manifest" || { echo "Manifeste introuvable : $manifest" >&2; exit 1; }

if test -z "${WINLUX_UPDATE_SIGNING_KEY:-}"; then
    if test "$required" = 1; then
        echo "WINLUX_UPDATE_SIGNING_KEY absent : manifeste non signé, les PC installés le refuseraient" >&2
        exit 1
    fi
    echo "::warning::Manifeste de mise à jour non signé (secret WINLUX_UPDATE_SIGNING_KEY absent)"
    exit 0
fi

seed=$(mktemp)
trap 'rm -f "$seed"' EXIT
chmod 600 "$seed"
printf '%s\n' "$WINLUX_UPDATE_SIGNING_KEY" >"$seed"
tool="$root/scripts/winlux-signature.py"
python3 -B "$tool" sign "$seed" "$manifest" >"$manifest.sig"
if test -f "$public"; then
    expected=$(tr -d ' \n' <"$public")
    actual=$(python3 -B "$tool" public "$seed")
    test "$expected" = "$actual" || {
        echo "La graine du secret ne correspond pas à integration/native/keys/update-signing.pub" >&2
        rm -f "$manifest.sig"; exit 1
    }
    python3 -B "$tool" verify "$public" "$manifest" "$manifest.sig"
fi
echo "Manifeste signé : $manifest.sig"
