#!/bin/sh
# Preserve the profile that installed a Windows program, including browser user.
set -eu
profile=${1:-desktop}
shift
# desktop-open: a file opened through a registry association. The program gets
# the document as a Windows path (C:\ is the home folder, Z:\ the root).
windows_path() {
    case "$1" in
        "$HOME"/*) printf 'C:%s' "${1#"$HOME"}" | sed 's@/@\\@g' ;;
        /*) printf 'Z:%s' "$1" | sed 's@/@\\@g' ;;
        *) printf '%s' "$1" ;;
    esac
}
case "$profile" in
    desktop) exec /usr/bin/winlux-open "$@" ;;
    desktop-open)
        exe=${1:?programme requis}
        shift
        for value in "$@"; do
            shift
            case "$value" in /*) if test -e "$value"; then value=$(windows_path "$value"); fi ;; esac
            set -- "$@" "$value"
        done
        exec /usr/bin/winlux-open "$exe" "$@" ;;
    browser)
        if test "$(id -u)" = 0; then
            /usr/bin/xhost +SI:localuser:winlux-web >/dev/null 2>&1 || true
            exec /usr/bin/setpriv --reuid=winlux-web --regid=winlux-web --init-groups --reset-env \
                env HOME=/home/winlux-web DISPLAY="${DISPLAY:-:0}" /usr/bin/winlux-open "$@"
        fi
        exec /usr/bin/winlux-open "$@" ;;
    *) exit 2 ;;
esac
