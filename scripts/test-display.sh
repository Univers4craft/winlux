#!/bin/sh
# Écrans multiples de bout en bout : vrai runtime, xrandr simulé avec un
# portable et un écran HDMI branché à chaud.
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-display-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
runtime="$project/target/debug/winlux"
if ! test -x "$runtime"; then
    (cd "$project" && cargo build -q -p winlux)
fi
mkdir -p "$work/bin" "$work/root"
ln -s "$runtime" "$work/bin/winlux"
export WINLUX_CONTROL_ROOT="$work/root" WINLUX_DISPLAY_TEST="$work"
export PATH="$work/bin:$PATH"
printf '0\n' >"$work/hdmi"
printf '1920x1080\n' >"$work/current"
cat >"$work/bin/xrandr" <<'MOCK'
#!/bin/sh
if test "$1" = --query; then
    printf 'Screen 0: minimum 320 x 200, current 1920 x 1080, maximum 16384 x 16384\n'
    printf 'Virtual-1 connected primary %s+0+0 (normal left) 0mm x 0mm\n' "$(cat "$WINLUX_DISPLAY_TEST/current")"
    current=$(cat "$WINLUX_DISPLAY_TEST/current")
    for mode in 1920x1080 1280x720 1024x768; do
        marker=; test "$mode" != 1920x1080 || marker=+
        test "$mode" != "$current" || marker="*$marker"
        printf '   %s 60.00%s\n' "$mode" "$marker"
    done
    if test "$(cat "$WINLUX_DISPLAY_TEST/hdmi")" = 1; then
        printf 'HDMI-1 connected (normal left) 597mm x 336mm\n'
        printf '   3840x2160 60.00+\n   1920x1080 60.00\n'
    else
        printf 'HDMI-1 disconnected (normal left)\n'
    fi
else
    printf '%s\n' "$*" >>"$WINLUX_DISPLAY_TEST/commands"
    mode=$(printf '%s\n' "$*" | sed -n 's/.*--output Virtual-1 --mode \([0-9x]*\).*/\1/p')
    test -z "$mode" || printf '%s\n' "$mode" >"$WINLUX_DISPLAY_TEST/current"
fi
MOCK
chmod 755 "$work/bin/xrandr"
display() { sh "$project/scripts/winlux-display.sh" "$@"; }
last() { tail -n 1 "$work/commands"; }
display status >"$work/status"
grep -qx 'display_modes=1920x1080,1280x720,1024x768' "$work/status"
grep -qx 'display_count=1' "$work/status"
# Déjà dans l'état voulu : aucun appel (pas de clignotement).
display sync
test ! -e "$work/commands"
display mode 1280x720
grep -qx 'POLICY=fixed' "$work/root/etc/winlux/display.conf"
grep -qx 'MODE=1280x720' "$work/root/etc/winlux/display.conf"
last | grep -q -- '--output Virtual-1 --mode 1280x720 --pos 0x0'
if display mode 9999x9999; then echo 'Définition non annoncée acceptée' >&2; exit 1; fi
display follow
last | grep -q -- '--mode 1920x1080'
display scale 1.5
grep -qx 'UI_SCALE=1.5' "$work/root/etc/winlux/display.conf"
if display scale '1;reboot'; then exit 1; fi
if display primary 'HDMI-1 --off'; then exit 1; fi
# Branchement à chaud d'un écran 4K : étendu à droite par défaut.
printf '1\n' >"$work/hdmi"
display sync
last | grep -q -- '--output HDMI-1 --mode 3840x2160 --pos 1920x0'
display status >"$work/status"
grep -qx 'display_count=2' "$work/status"
grep -qx 'display_2_name=HDMI-1' "$work/status"
display layout duplicate
last | grep -q -- '--output HDMI-1 --mode 1920x1080 --pos 0x0'
display layout extend
display place HDMI-1 left
display primary HDMI-1
grep -qx 'display.HDMI-1+Virtual-1.primary=HDMI-1' "$work/root/etc/winlux/display.conf"
last | grep -q -- '--output HDMI-1 .*--pos 0x0 .*--primary'
display layout only HDMI-1
last | grep -q -- '^--output Virtual-1 --off'
display scale HDMI-1 2
grep -qx 'display.HDMI-1+Virtual-1.scale.HDMI-1=2' "$work/root/etc/winlux/display.conf"
test "$(display session-scale)" = 2
# Débranché : l'écran seul retrouve son propre réglage.
printf '0\n' >"$work/hdmi"
display sync
last | grep -q -- '--output Virtual-1 --mode 1920x1080 --pos 0x0'
echo 'OK: écrans multiples, branchement à chaud, disposition, principal, échelle par écran et validation'
