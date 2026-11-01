#!/bin/sh
# Pavé tactile : capacités lues sur xinput (simulé), réglages appliqués à
# tous les pavés, mémorisés et réappliqués ; rien sans pavé tactile.
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-touchpad-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/bin" "$work/root"
export WINLUX_CONTROL_ROOT="$work/root" WINLUX_TP_TEST="$work" PATH="$work/bin:$PATH"
printf 'tap=0\nnatural=0\nscroll=0, 1, 0\nclick=1, 0\ndwt=1\naccel=0.000000\n' >"$work/state"
printf '1\n' >"$work/present"
cat >"$work/bin/xinput" <<'MOCK'
#!/bin/sh
s="$WINLUX_TP_TEST/state"
v() { sed -n "s/^$1=//p" "$s"; }
case "$1" in
    --version) printf 'xinput version 1.6.4\nXI version on server: 2.4\n' ;;
    --list) printf '2\n9\n'; test "$(cat "$WINLUX_TP_TEST/present")" = 0 || printf '12\n' ;;
    list-props)
        test "$2" = 12 || { printf "Device 'Mouse':\n\tlibinput Accel Speed (300):\t0.000000\n"; exit 0; }
        printf "Device 'SYNA Touchpad':\n"
        printf '\tlibinput Tapping Enabled (310):\t%s\n' "$(v tap)"
        printf '\tlibinput Natural Scrolling Enabled (320):\t%s\n' "$(v natural)"
        printf '\tlibinput Scroll Methods Available (330):\t1, 1, 0\n'
        printf '\tlibinput Scroll Method Enabled (331):\t%s\n' "$(v scroll)"
        printf '\tlibinput Click Methods Available (340):\t1, 1\n'
        printf '\tlibinput Click Method Enabled (341):\t%s\n' "$(v click)"
        printf '\tlibinput Disable While Typing Enabled (350):\t%s\n' "$(v dwt)"
        printf '\tlibinput Accel Speed (360):\t%s\n' "$(v accel)" ;;
    set-prop)
        test "$2" = 12 || { echo "set-prop sur un autre périphérique : $*" >&2; exit 1; }
        prop=$3; shift 3; val=$(printf '%s' "$*" | sed 's/ /, /g')
        case "$prop" in
            'libinput Tapping Enabled') key=tap ;; 'libinput Natural Scrolling Enabled') key=natural ;;
            'libinput Scroll Method Enabled') key=scroll ;; 'libinput Click Method Enabled') key=click ;;
            'libinput Disable While Typing Enabled') key=dwt ;; 'libinput Accel Speed') key=accel ;;
            *) exit 1 ;;
        esac
        sed -i "s/^$key=.*/$key=$val/" "$s" ;;
esac
MOCK
chmod 755 "$work/bin/xinput"
tp() { sh "$project/scripts/winlux-touchpad.sh" "$@"; }
tp status >"$work/status"
for line in touchpad_present=1 touchpad_tap=0 touchpad_natural=0 touchpad_twofinger=0 touchpad_clickfinger=0 touchpad_dwt=1 touchpad_speed=50 touchpad_gestures=1; do
    grep -qx "$line" "$work/status" || { echo "$line absent" >&2; cat "$work/status" >&2; exit 1; }
done
tp set twofinger 1
tp set clickfinger 1
tp set tap 1
tp set natural 1
tp set speed 75
tp status >"$work/status"
for line in touchpad_tap=1 touchpad_natural=1 touchpad_twofinger=1 touchpad_clickfinger=1 touchpad_speed=75; do
    grep -qx "$line" "$work/status" || { echo "$line absent" >&2; cat "$work/status" >&2; exit 1; }
done
grep -qx 'TAP_TO_CLICK=1' "$work/root/etc/winlux/input.conf"
grep -qx 'TOUCHPAD_TWOFINGER=1' "$work/root/etc/winlux/input.conf"
grep -qx 'TOUCHPAD_SPEED=75' "$work/root/etc/winlux/input.conf"
# Pavé réinitialisé (reprise de veille, rebranchement) : tout revient.
printf 'tap=0\nnatural=0\nscroll=0, 1, 0\nclick=1, 0\ndwt=1\naccel=0.000000\n' >"$work/state"
tp apply
tp status | grep -qx touchpad_twofinger=1
tp status | grep -qx touchpad_tap=1
# Entrées refusées.
if tp set tap 2; then exit 1; fi
if tp set speed 101; then exit 1; fi
if tp set 'tap;reboot' 1; then exit 1; fi
# Ordinateur fixe : aucun pavé, la section est cachée.
printf '0\n' >"$work/present"
test "$(tp status)" = touchpad_present=0
if tp set tap 1 2>/dev/null; then exit 1; fi
echo 'OK: pavé tactile détecté, défilement/clic à deux doigts, naturel, vitesse, réapplication et absence'
