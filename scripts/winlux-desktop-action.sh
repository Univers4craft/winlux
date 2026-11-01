#!/bin/sh
set -eu
case "${1:-}" in
    context) exec /usr/bin/xdotool key --clearmodifiers super+F10 ;;
    menu) exec /usr/bin/winlux-start-menu ;;
    refresh) /usr/bin/winlux-applications refresh; exec /usr/bin/xrefresh ;;
    paste) exec /usr/bin/winlux-desktop-paste ;;
    folder|text)
        directory="${HOME:-/root}/Bureau"; mkdir -p "$directory"
        if test "$1" = folder; then name='Nouveau dossier'; suffix=; else name='Nouveau document'; suffix=.txt; fi
        n=0
        while :; do
            if test "$n" = 0; then target="$directory/$name$suffix"; else target="$directory/$name ($n)$suffix"; fi
            if test "$1" = folder; then
                if mkdir "$target" 2>/dev/null; then break; fi
            else
                if (set -C; : > "$target") 2>/dev/null; then break; fi
            fi
            n=$((n + 1)); test "$n" -lt 1000 || exit 1
        done
        if test "$1" = text; then exec /usr/bin/mousepad "$target"; fi
        exec /usr/bin/winlux-shell-app explorer "$directory" ;;
    screensaver) exec /usr/bin/xscreensaver-command -activate ;;
    screensaver-settings) exec /usr/bin/xscreensaver-settings ;;
    # Boîte de confirmation au style Windows 11, puis arrêt réel (winlux-power).
    shutdown|reboot) exec /usr/bin/winlux-power ask "$1" ;;
    lock) exec /usr/bin/winlux-lock now ;;
    *) exit 2 ;;
esac
