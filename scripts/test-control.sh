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
case "$1" in --list) echo 12 ;; list-props) printf 'libinput Tapping Enabled (100): 0\nlibinput Accel Speed (101): 0\n' ;; set-prop) printf 'xinput %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG" ;; esac
MOCK
cat > "$work/bin/winlux-update" <<'MOCK'
#!/bin/sh
case "$*" in --busy) test "${WINLUX_TEST_BUSY:-0}" = 1 ;; '--auto status') echo enabled ;; --list) echo 'desktop v1 -> v2' ;; *) printf 'update %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG"; echo 'Update complete' ;; esac
MOCK
cat > "$work/bin/winlux-network" <<'MOCK'
#!/bin/sh
printf 'network %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG"
MOCK
cat > "$work/bin/apt-get" <<'MOCK'
#!/bin/sh
case "$*" in
  '-s -q upgrade') echo 'Inst libfoo1 [1.0-1] (1.0-2 Ubuntu:26.04/resolute-updates [amd64])' ;;
  *) printf 'apt-get %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG" ;;
esac
MOCK
cat > "$work/bin/id" <<'MOCK'
#!/bin/sh
echo 0
MOCK
printf '#!/bin/sh\nexec sh "%s/scripts/winlux-sysupdate.sh" "$@"\n' "$project" > "$work/bin/winlux-sysupdate"
chmod 755 "$work/bin/"*
control() { sh "$project/scripts/winlux-control.sh" "$@"; }
control status
mkdir -p "$work/root/var/lib/winlux"
printf 'test-boot\n' > "$work/root/var/lib/winlux/pending-reboot"
control status
grep -qx 'update_action=reboot' "$work/root/run/winlux/control.txt"
rm "$work/root/var/lib/winlux/pending-reboot"
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
control mouse-fast
grep -qx 'MOUSE_SPEED=75' "$work/root/etc/winlux/input.conf"
grep -qx 'xinput set-prop 12 libinput Accel Speed 0.5' "$work/commands"
grep -qx 'mouse_speed=75' "$work/root/run/winlux/control.txt"
control mouse-slow
grep -qx 'MOUSE_SPEED=25' "$work/root/etc/winlux/input.conf"
grep -q '^XKB_LAYOUT=us$' "$work/root/etc/winlux/input.conf"
grep -q '^setxkbmap -layout us$' "$work/commands"
if WINLUX_TEST_KEYBOARD_FAIL=1 control keyboard-fr; then echo 'ERROR: failed-layout accepted' >&2; exit 1; fi
grep -q '^XKB_LAYOUT=us$' "$work/root/etc/winlux/input.conf"
control tap-on
grep -q '^TAP_TO_CLICK=1$' "$work/root/etc/winlux/input.conf"
cat > "$work/bin/xfconf-query" <<'MOCK'
#!/bin/sh
printf 'xfconf %s\n' "$*" >> "$WINLUX_CONTROL_TEST_LOG"
case " $* " in *' -s '*) exit 0 ;; *) exit 1 ;; esac
MOCK
chmod 755 "$work/bin/xfconf-query"
control align-center
grep -qx 'TASKBAR_ALIGN=center' "$work/root/etc/winlux/preferences.conf"
grep -q 'xfconf -c xfce4-panel -p /plugins/plugin-3/expand -s true' "$work/commands"
control align-left
grep -q 'xfconf -c xfce4-panel -p /plugins/plugin-3/expand -s false' "$work/commands"
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
grep -qx 'welcome_done=0' "$work/root/run/winlux/control.txt"
control welcome-done
grep -qx 'welcome_done=1' "$work/root/run/winlux/control.txt"
grep -qx 'privacy_telemetry=off' "$work/root/run/winlux/control.txt"
grep -qx 'privacy_location=deny' "$work/root/run/winlux/control.txt"
grep -qx 'privacy_location_history=off' "$work/root/run/winlux/control.txt"
control privacy-microphone-deny
grep -qx 'MICROPHONE=deny' "$work/root/etc/winlux/privacy.conf"
grep -qx 'privacy_microphone=deny' "$work/root/run/winlux/control.txt"
grep -qx 'sysupdate_mode=manual' "$work/root/run/winlux/control.txt"
control sysupdate-mode-scheduled
grep -qx 'sysupdate_mode=scheduled' "$work/root/run/winlux/control.txt"
grep -qx 'update --auto disable' "$work/commands"
control sysupdate-hour-up
grep -qx 'sysupdate_hour=4' "$work/root/run/winlux/control.txt"
control sysupdate-day-next
grep -qx 'sysupdate_day=mon' "$work/root/run/winlux/control.txt"
control sysupdate-mode-auto
grep -qx 'update --auto enable' "$work/commands"
control sysupdate-check
grep -qx 'sysupdate_count=1' "$work/root/run/winlux/control.txt"
grep -qx 'libfoo1 1.0-1 -> 1.0-2' "$work/root/run/winlux/sysupdates.txt"
control sysupdate-apply
grep -q '^apt-get -q -y .* upgrade$' "$work/commands"
grep -qx 'sysupdate_count=0' "$work/root/run/winlux/control.txt"
touch "$work/root/etc/winlux/live-session"
if WINLUX_TEST_BUSY=1 control poweroff; then echo 'ERROR: shutdown allowed during update' >&2; exit 1; fi
if WINLUX_TEST_BUSY=1 control reboot; then echo 'ERROR: reboot allowed during update' >&2; exit 1; fi
if WINLUX_TEST_BUSY=1 control restart-session; then echo 'ERROR: session restart allowed during update' >&2; exit 1; fi
rm "$work/root/etc/winlux/live-session"
if control reboot; then echo 'ERROR: refused-host-reboot failed' >&2; exit 1; fi
grep -q 'Arret reserve a Winlux Live' "$work/root/run/winlux/action.log"
if control 'keyboard-fr;reboot'; then echo 'ERROR: invalid-action failed' >&2; exit 1; fi
echo 'OK: control telemetry, keyboard, touchpad, preferences, actions, update journal, system updates, privacy, welcome, host protection'
