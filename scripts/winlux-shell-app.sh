#!/bin/sh
set -eu
export WINLUX_NATIVE_SHELL=1
unset WINLUX_DESKTOP_DEMO
case "${1:-explorer}" in
    explorer|settings)
        export WINLUX_SHELL_APP=$1
        if test "$1" = settings && test -n "${2:-}"; then
            case "$2" in *[!0-9]*) exit 2 ;; esac
            export WINLUX_SETTINGS_PAGE=$2
        fi
        if test "$1" = explorer && test -n "${2:-}"; then
            case "$2" in /*) WINLUX_SHELL_PATH="Z:$(printf '%s' "$2" | sed 's@/@\\@g')" ;; *) WINLUX_SHELL_PATH=$2 ;; esac
            export WINLUX_SHELL_PATH
        fi
        if test "$1" = settings && test "${WINLUX_LEGACY_SETTINGS:-0}" != 1; then
            exec /usr/bin/python3 /usr/bin/winlux-settings
        fi
        exec /usr/bin/winlux /opt/winlux/tests/gui_demo.exe ;;
    store|fonts|programs|taskschd|firewall|welcome|regedit|taskmgr|eventviewer)
        export WINLUX_SHELL_APP=$1
        exec /usr/bin/winlux /opt/winlux/tests/gui_demo.exe ;;
    editor) shift; exec /usr/bin/mousepad "$@" ;;
    *) echo 'Application Winlux inconnue' >&2; exit 2 ;;
esac
