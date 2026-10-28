#!/bin/sh
# Preserve the profile that installed a Windows program, including browser user.
set -eu
profile=${1:-desktop}
shift
case "$profile" in
    desktop) exec /usr/bin/winlux-open "$@" ;;
    browser)
        if test "$(id -u)" = 0; then
            /usr/bin/xhost +SI:localuser:winlux-web >/dev/null 2>&1 || true
            exec /usr/bin/setpriv --reuid=winlux-web --regid=winlux-web --init-groups --reset-env \
                env HOME=/home/winlux-web DISPLAY="${DISPLAY:-:0}" /usr/bin/winlux-open "$@"
        fi
        exec /usr/bin/winlux-open "$@" ;;
    *) exit 2 ;;
esac
