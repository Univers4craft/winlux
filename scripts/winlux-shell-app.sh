#!/bin/sh
set -eu
export WINLUX_NATIVE_SHELL=1
unset WINLUX_DESKTOP_DEMO

# Chemin passé à l'Explorateur. xdg-open et GIO donnent parfois une URI
# (file:///home/damien/T%C3%A9l%C3%A9chargements) : on la décode (accents,
# espaces, # ? % …) en chemin Linux, puis /a/b devient Z:\a\b.
shell_path() {
    value=$1
    case "$value" in
        [Ff][Ii][Ll][Ee]:*)
            value=$(/usr/bin/python3 -I -c '
import os, sys
from urllib.parse import unquote_to_bytes, urlsplit
uri = urlsplit(sys.argv[1])
if uri.netloc not in ("", "localhost") or not uri.path.startswith("/"):
    sys.exit(1)
path = unquote_to_bytes(os.fsencode(uri.path))
if b"\0" in path:
    sys.exit(1)
sys.stdout.buffer.write(path)
' "$value") || return 1 ;;
    esac
    case "$value" in
        /*) printf 'Z:%s' "$value" | sed 's@/@\\@g' ;;
        *) printf '%s' "$value" ;;
    esac
}

if test "${1:-}" = --chemin; then
    shell_path "${2:-}"
    exit
fi
case "${1:-explorer}" in
    explorer|settings)
        export WINLUX_SHELL_APP=$1
        if test "$1" = settings && test -n "${2:-}"; then
            # Nom de page (about, display…) : traduit par winlux-settings.
            case "$2" in
                *[!0-9]*)
                    case "$2" in *[!A-Za-z0-9:_-]*) exit 2 ;; esac
                    test "${WINLUX_LEGACY_SETTINGS:-0}" != 1 || exit 2
                    exec /usr/bin/python3 /usr/bin/winlux-settings "$2" ;;
            esac
            export WINLUX_SETTINGS_PAGE=$2
        fi
        if test "$1" = explorer && test -n "${2:-}"; then
            WINLUX_SHELL_PATH=$(shell_path "$2") || exit 2
            export WINLUX_SHELL_PATH
        fi
        if test "$1" = settings && test "${WINLUX_LEGACY_SETTINGS:-0}" != 1; then
            exec /usr/bin/python3 /usr/bin/winlux-settings
        fi
        exec /usr/bin/winlux /opt/winlux/tests/gui_demo.exe ;;
    store|fonts|programs|taskschd|firewall|welcome|regedit|taskmgr|eventviewer|msinfo32)
        export WINLUX_SHELL_APP=$1
        # Bureau natif : « Installer ou désinstaller des programmes » est la page
        # Applications installées des Paramètres, filtrée sur Windows, avec les
        # vraies icônes des .exe (#756). La fenêtre PE reste pour le shell PE.
        if test "$1" = programs && test "${WINLUX_LEGACY_SETTINGS:-0}" != 1 && test -x /usr/bin/winlux-installed-apps &&
           test -f /usr/bin/winlux-settings; then
            export WINLUX_INSTALLED_FILTER=windows
            exec /usr/bin/python3 /usr/bin/winlux-settings installed-apps
        fi
        # Lanceur d'une boutique (winlux-store-catalogs) : le Store n'affiche
        # que ses logiciels, sous son nom (« flatpak: Flathub »).
        if test "$1" = store && test -n "${2:-}"; then
            case "$2" in *[!a-z0-9:/-]*|[!a-z]*) exit 2 ;; esac
            case "${3:-}" in *[!A-Za-z0-9_]*) exit 2 ;; esac
            export WINLUX_STORE_SOURCE=$2
            WINLUX_STORE_TITLE=$(printf '%s' "${3:-}" | tr _ ' ')
            export WINLUX_STORE_TITLE
        fi
        # Dossier d'échange du Store propre au compte : partagé, l'index écrit
        # par un premier compte ne pouvait pas être remplacé par un second
        # (/run/winlux est « sticky ») et son Store restait vide.
        if test "$1" = store && test "$(id -u)" != 0; then
            uid=$(id -u)
            for dir in "/run/winlux/store-$uid" "${XDG_RUNTIME_DIR:-/nonexistent}/winlux-store" "/tmp/winlux-store-$uid"; do
                case "$dir" in *[!A-Za-z0-9/_.-]*|/nonexistent/*) continue ;; esac
                mkdir -p -m 700 "$dir" 2>/dev/null || true
                if test -d "$dir" && test -O "$dir" && test -w "$dir" && test ! -L "$dir"; then
                    export WINLUX_STORE_RUN="$dir"
                    break
                fi
            done
        fi
        exec /usr/bin/winlux /opt/winlux/tests/gui_demo.exe ;;
    editor) shift; exec /usr/bin/mousepad "$@" ;;
    *) echo 'Application Winlux inconnue' >&2; exit 2 ;;
esac
