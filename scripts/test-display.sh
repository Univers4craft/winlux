#!/bin/sh
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-display-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/bin" "$work/root"
export WINLUX_CONTROL_ROOT="$work/root" WINLUX_DISPLAY_TEST="$work"
export PATH="$work/bin:$PATH"
printf '1920x1080\n' >"$work/current"
cat >"$work/bin/xrandr" <<'MOCK'
#!/bin/sh
if test "$1" = --query; then
    current=$(cat "$WINLUX_DISPLAY_TEST/current")
    printf 'Virtual-1 connected primary\n'
    for mode in 1920x1080 1280x720 1024x768; do
        marker=; test "$mode" != 1920x1080 || marker=+
        test "$mode" != "$current" || marker="*$marker"
        printf '   %s 60.00%s\n' "$mode" "$marker"
    done
else
    printf '%s\n' "$*" >>"$WINLUX_DISPLAY_TEST/commands"
    case "$*" in *--mode*) printf '%s\n' "$4" >"$WINLUX_DISPLAY_TEST/current" ;; *--auto*) printf '1920x1080\n' >"$WINLUX_DISPLAY_TEST/current" ;; esac
fi
MOCK
chmod 755 "$work/bin/xrandr"
display() { sh "$project/scripts/winlux-display.sh" "$@"; }
display status >"$work/status"
grep -qx 'display_modes=1920x1080,1280x720,1024x768' "$work/status"
display mode 1280x720
grep -qx 'POLICY=fixed' "$work/root/etc/winlux/display.conf"
grep -qx 'MODE=1280x720' "$work/root/etc/winlux/display.conf"
display sync
test "$(cat "$work/current")" = 1280x720
if display mode 9999x9999; then echo 'Unadvertised resolution accepted' >&2; exit 1; fi
test "$(cat "$work/current")" = 1280x720
display follow
test "$(cat "$work/current")" = 1920x1080
grep -qx 'POLICY=follow' "$work/root/etc/winlux/display.conf"
display scale 1.5
display scale auto
grep -qx 'UI_SCALE=auto' "$work/root/etc/winlux/display.conf"
if display scale '1;reboot'; then exit 1; fi
echo 'OK: advertised modes, fixed/follow policy, persistence, scale and input validation'
