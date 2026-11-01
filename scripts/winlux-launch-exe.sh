#!/bin/sh
# Preserve the profile that installed a Windows program, including browser user.
set -eu
profile=${1:-desktop}
# Sans argument, shift échouerait (set -e) : le profil par défaut suffit.
test $# -eq 0 || shift
# desktop-open: a file opened through a registry association. The program gets
# the document as a Windows path (C:\ is the drive_c folder, Z:\ the root).
# Même correspondance que le runtime (paths.rs) : C:\ est WINLUX_DRIVE_C,
# sinon WINLUX_HOME, sinon HOME ; tout autre chemin absolu est sous Z:\.
drive_c=${WINLUX_DRIVE_C:-${WINLUX_HOME:-${HOME:-/}}}
drive_c=${drive_c%/}
windows_path() {
    case "$1" in
        "$drive_c") printf 'C:\\' ;;
        "$drive_c"/*) printf 'C:%s' "${1#"$drive_c"}" | sed 's@/@\\@g' ;;
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
            # Ses téléchargements (FOLDERID_Downloads) vont dans ceux de l'utilisateur.
            downloads=/usr/share/winlux/native/browser/winlux-browser-downloads.py
            test ! -f "$downloads" || /usr/bin/python3 "$downloads" prepare >/dev/null 2>&1 || true
            exec /usr/bin/setpriv --reuid=winlux-web --regid=winlux-web --init-groups --reset-env \
                env HOME=/home/winlux-web DISPLAY="${DISPLAY:-:0}" /usr/bin/winlux-open "$@"
        fi
        exec /usr/bin/winlux-open "$@" ;;
    *) exit 2 ;;
esac
