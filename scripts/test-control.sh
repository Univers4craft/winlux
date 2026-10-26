#!/bin/sh
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-control-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/bin" "$work/root/etc/winlux" "$work/root/usr/share/winlux" "$work/root/proc/bus/input" "$work/root/sys/class/power_supply/BAT0"
export WINLUX_CONTROL_ROOT="$work/root"
export WINLUX_CONTROL_TEST_LOG="$work/commands"
export PATH="$work/bin:$PATH"
printf 'XKB_LAYOUT=fr\nTAP_TO_CLICK=0\n' > "$work/root/etc/winlux/input.conf"
printf 'v-test\n' > "$work/root/usr/share/winlux/version"
printf '84\n' > "$work/root/sys/class/power_supply/BAT0/capacity"
printf 'Charging\n' > "$work/root/sys/class/power_supply/BAT0/status"
printf 'N: Name="Tablet"\nN: Name="Keyboard"\n' > "$work/root/proc/bus/input/devices"
printf '/dev/vda / ext4 rw 0 0\n' > "$work/root/proc/mounts"
cat > "$work/bin/ip" <<'MOCK'
#!/bin/sh
case "$*" in
  '-4 route show default') echo 'default via 10.0.2.2 dev eth0' ;;
  '-4 -o addr show dev eth0') echo '2: eth0 inet 10.0.2.15/24 brd 10.0.2.255 scope global eth0' ;;
esac
MOCK
cat > "$work/bin/wpctl" <<'MOCK'
#!/bin/sh
case "$1" in get-volume) echo 'Volume: 0.45' ;; *) printf 'wpctl %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG" ;; esac
MOCK
cat > "$work/bin/setxkbmap" <<'MOCK'
#!/bin/sh
test "${WINLUX_TEST_KEYBOARD_FAIL:-0}" != 1 || exit 1
printf 'setxkbmap %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG"
MOCK
cat > "$work/bin/xinput" <<'MOCK'
#!/bin/sh
case "$1" in --list) echo 12 ;; list-props) echo 'libinput Tapping Enabled (100): 0' ;; set-prop) printf 'xinput %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG" ;; esac
MOCK
cat > "$work/bin/winlux-update" <<'MOCK'
#!/bin/sh
case "$*" in --busy) test "${WINLUX_TEST_BUSY:-0}" = 1 ;; '--auto status') echo enabled ;; --list) echo 'desktop v1 -> v2' ;; *) printf 'update %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG"; echo 'Update complete' ;; esac
MOCK
cat > "$work/bin/winlux-network" <<'MOCK'
#!/bin/sh
printf 'network %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG"
MOCK
cat > "$work/bin/id" <<'MOCK'
#!/bin/sh
case "$1" in -u) echo 0 ;; *) command id "$@" ;; esac
MOCK
chmod 755 "$work/bin/"*
control() { sh "$project/scripts/winlux-control.sh" "$@"; }
control status
grep -q '^network_connected=1$' "$work/root/run/winlux/control.txt"
grep -q '^network=eth0 10.0.2.15/24$' "$work/root/run/winlux/control.txt"
grep -q '^battery=84% Charging$' "$work/root/run/winlux/control.txt"
grep -q '^input_devices=2$' "$work/root/run/winlux/control.txt"
grep -q '^persistent=1$' "$work/root/run/winlux/control.txt"
grep -q '^version=v-test$' "$work/root/run/winlux/control.txt"
printf 'rootfs / rootfs rw 0 0\n' > "$work/root/proc/mounts"
control status
grep -q '^persistent=0$' "$work/root/run/winlux/control.txt"
grep -q '^audio=Volume: 0.45$' "$work/root/run/winlux/control.txt"
control keyboard-us
grep -q '^XKB_LAYOUT=us$' "$work/root/etc/winlux/input.conf"
grep -q '^setxkbmap -layout us$' "$work/commands"
if WINLUX_TEST_KEYBOARD_FAIL=1 control keyboard-fr; then echo 'ERROR: failed-layout accepted' >&2; exit 1; fi
grep -q '^XKB_LAYOUT=us$' "$work/root/etc/winlux/input.conf"
control tap-on
grep -q '^TAP_TO_CLICK=1$' "$work/root/etc/winlux/input.conf"
control align-left
control theme-light
grep -q '^TASKBAR_ALIGN=left$' "$work/root/etc/winlux/preferences.conf"
grep -q '^THEME=light$' "$work/root/etc/winlux/preferences.conf"
control network-renew
grep -q '^network restart$' "$work/commands"
control volume-down
grep -q '^wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-$' "$work/commands"
control update-check
grep -q '^desktop v1 -> v2$' "$work/root/run/winlux/updates.txt"
control update-apply
grep -q '^Update complete$' "$work/root/run/winlux/update.log"
touch "$work/root/etc/winlux/live-session"
if WINLUX_TEST_BUSY=1 control poweroff; then echo 'ERROR: shutdown allowed during update' >&2; exit 1; fi
if WINLUX_TEST_BUSY=1 control reboot; then echo 'ERROR: reboot allowed during update' >&2; exit 1; fi
if WINLUX_TEST_BUSY=1 control restart-session; then echo 'ERROR: session restart allowed during update' >&2; exit 1; fi
rm "$work/root/etc/winlux/live-session"
printf '%s' 'ghp_testOnlyToken123' > "$work/root/run/winlux/update-token-request"
control update-save-token
test ! -e "$work/root/run/winlux/update-token-request"
test "$(stat -c %a "$work/root/etc/winlux/update.token")" = 600
printf '%s\n' 'ghp_testOnlyToken123' > "$work/expected-token"
cmp -s "$work/expected-token" "$work/root/etc/winlux/update.token"
grep -q '^update_auth=configured$' "$work/root/run/winlux/control.txt"
if grep -rq 'ghp_testOnlyToken123' "$work/root/run" "$work/commands"; then echo 'ERROR: token leaked into state or logs' >&2; exit 1; fi
: > "$work/root/run/winlux/update-token-request"
control update-save-token
test ! -e "$work/root/etc/winlux/update.token"
grep -q '^update_auth=missing$' "$work/root/run/winlux/control.txt"
printf '%s' 'invalid secret with spaces' > "$work/root/run/winlux/update-token-request"
if control update-save-token; then echo 'ERROR: invalid token accepted' >&2; exit 1; fi
test ! -e "$work/root/run/winlux/update-token-request"
if grep -rq 'invalid secret with spaces' "$work/root/run"; then echo 'ERROR: invalid token logged' >&2; exit 1; fi
if control reboot; then echo 'ERROR: refused-host-reboot failed' >&2; exit 1; fi
grep -q 'Arret reserve a Winlux Live' "$work/root/run/winlux/action.log"
if control 'keyboard-fr;reboot'; then echo 'ERROR: invalid-action failed' >&2; exit 1; fi
echo 'OK: control telemetry, keyboard, touchpad, preferences, actions, update journal, private token lifecycle, host protection'
