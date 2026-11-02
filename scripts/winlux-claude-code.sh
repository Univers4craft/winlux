#!/bin/sh
# Installe Claude Code dans Winlux, après le premier démarrage (rien dans l'ISO).
#
#   winlux-claude-code install          version Windows (claude.exe) lancée par
#                                       la couche PE maison de Winlux
#   winlux-claude-code install linux    version Linux native (solution de repli)
#   winlux-claude-code version          version installée
#
# Le binaire officiel est téléchargé depuis le dépôt de diffusion d'Anthropic,
# vérifié avec la somme SHA-256 de son manifeste, puis rangé dans
# ~/.local/share/winlux/claude-code. La commande « claude » (~/.local/bin)
# le lance. Claude Code demande ensuite de se connecter (navigateur) au
# premier lancement.
set -eu

BASE="https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}/winlux/claude-code"
BIN="$HOME/.local/bin"
CANAL="${WINLUX_CLAUDE_CANAL:-stable}"

die() { echo "winlux-claude-code : $*" >&2; exit 1; }

fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        die "ni curl ni wget n'est installé"
    fi
}

install_claude() {
    mode="${1:-windows}"
    case "$mode" in
        windows) plateforme=win32-x64 ; fichier=claude.exe ;;
        linux) plateforme=linux-x64 ; fichier=claude ;;
        *) die "mode inconnu : $mode (windows ou linux)" ;;
    esac
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/winlux-claude.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT
    fetch "$BASE/$CANAL" "$tmp/version"
    version=$(tr -d ' \r\n' <"$tmp/version")
    case "$version" in
        [0-9]*.[0-9]*.[0-9]*) ;;
        *) die "version introuvable sur le canal $CANAL" ;;
    esac
    fetch "$BASE/$version/manifest.json" "$tmp/manifest.json"
    somme=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["platforms"][sys.argv[2]]["checksum"])' \
        "$tmp/manifest.json" "$plateforme") || die "plateforme $plateforme absente du manifeste"
    echo "Téléchargement de Claude Code $version ($plateforme)…"
    fetch "$BASE/$version/$plateforme/$fichier" "$tmp/$fichier"
    echo "$somme  $tmp/$fichier" | sha256sum -c --status || die "somme SHA-256 incorrecte, fichier rejeté"
    mkdir -p "$DATA" "$BIN"
    install -m 0755 "$tmp/$fichier" "$DATA/$fichier"
    echo "$version $mode" >"$DATA/version"
    if [ "$mode" = windows ]; then
        cat >"$BIN/claude" <<EOF
#!/bin/sh
# Claude Code (version Windows) via la couche PE de Winlux.
exec winlux "$DATA/claude.exe" "\$@"
EOF
    else
        cat >"$BIN/claude" <<EOF
#!/bin/sh
# Claude Code (version Linux native).
exec "$DATA/claude" "\$@"
EOF
    fi
    chmod 0755 "$BIN/claude"
    echo "Claude Code $version installé. Lancez « claude » dans un terminal."
    case ":$PATH:" in
        *":$BIN:"*) ;;
        *) echo "Ajoutez $BIN au PATH, ou lancez $BIN/claude." ;;
    esac
}

case "${1:-}" in
    install) install_claude "${2:-windows}" ;;
    version) cat "$DATA/version" 2>/dev/null || echo "non installé" ;;
    *) echo "usage : winlux-claude-code install [windows|linux] | version" >&2; exit 2 ;;
esac
