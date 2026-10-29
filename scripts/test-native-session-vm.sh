#!/bin/sh
# Real services and windows on a disk created only for this diagnostic.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
stage=${WINLUX_NATIVE_STAGE:-"$root/build/winlux-live-test/initramfs-root"}
work=$(mktemp -d "$root/build/native-vm.XXXXXX")
disk="$work/test.raw"
log="$work/serial.log"
accel=tcg
if test -r /dev/kvm && test -w /dev/kvm; then accel=kvm; fi
truncate -s 10G "$disk"
fakeroot sh -c 'chown -R 0:0 "$1"; mkfs.ext4 -q -F -L WINLUX_NATIVE -d "$1" "$2"' sh "$stage" "$disk"
if test -n "${WINLUX_EXTERNAL_EXE:-}"; then
    test -f "$WINLUX_EXTERNAL_EXE" && test -r "$WINLUX_EXTERNAL_EXE"
    debugfs -w -R 'mkdir /opt/winlux-test' "$disk" >/dev/null 2>&1
    # Fixed destination; debugfs syntax is not a shell command interpreter.
    case "$WINLUX_EXTERNAL_EXE" in *'"'*|*'\\'*|*'
'*) echo 'unsupported diagnostic filename' >&2; exit 1 ;; esac
    app_name=$(basename -- "$WINLUX_EXTERNAL_EXE")
    case "$app_name" in *[!A-Za-z0-9._-]*) echo 'unsupported vendor executable name' >&2; exit 1 ;; esac
    printf '%s\n' "$app_name" > "$work/executable-name.txt"
    debugfs -w -R "write $work/executable-name.txt /opt/winlux-test/executable-name.txt" "$disk" >/dev/null 2>&1
    debugfs -w -R "write \"$WINLUX_EXTERNAL_EXE\" /opt/winlux-test/$app_name" "$disk" >/dev/null 2>&1
    if test "${WINLUX_EXTERNAL_DLLS:-0}" = 1; then
        for library in "$(dirname -- "$WINLUX_EXTERNAL_EXE")"/*.dll; do
            test -f "$library" || continue
            name=$(basename -- "$library")
            case "$name" in *[!A-Za-z0-9._-]*) echo 'unsupported vendor DLL name' >&2; exit 1 ;; esac
            debugfs -w -R "write \"$library\" /opt/winlux-test/$name" "$disk" >/dev/null 2>&1
        done
    fi
fi
echo "Native desktop diagnostic ($accel): $work"
result=0
timeout -k 5s "${WINLUX_NATIVE_TEST_TIMEOUT:-600}" qemu-system-x86_64 \
    -m "${WINLUX_QEMU_MEMORY:-4096}" -smp 2 -accel "$accel" \
    -display none -monitor none -serial stdio -no-reboot -nic none \
    -kernel "$stage/boot/vmlinuz" \
    -append 'root=/dev/vda rw rootfstype=ext4 init=/init console=ttyS0 panic=1 loglevel=3 winlux.native-smoke=1' \
    -drive "file=$disk,if=virtio,format=raw" > "$log" 2>&1 || result=$?
for name in native-desktop native-start-menu native-start-all native-start-search native-settings-updates native-vendor-external native-vendor-chatgpt native-vendor-claude native-pipes-1 native-pipes-2; do
    debugfs -R "dump /var/lib/winlux/$name.xwd $work/$name.xwd" "$disk" >/dev/null 2>&1 || true
    if test -s "$work/$name.xwd" && command -v convert >/dev/null 2>&1; then
        convert "$work/$name.xwd" "$work/$name.png"
    elif test -s "$work/$name.xwd" && command -v ffmpeg >/dev/null 2>&1; then
        ffmpeg -hide_banner -loglevel error -i "$work/$name.xwd" -frames:v 1 "$work/$name.png"
    fi
done
debugfs -R "dump /var/lib/winlux/native-programs.log $work/programs.log" "$disk" >/dev/null 2>&1 || true
debugfs -R "dump /var/lib/winlux/chatgpt-result.txt $work/chatgpt-result.txt" "$disk" >/dev/null 2>&1 || true
debugfs -R "dump /var/lib/winlux/vendor-result.txt $work/vendor-result.txt" "$disk" >/dev/null 2>&1 || true
debugfs -R "dump /var/lib/winlux/vendor-results.json $work/vendor-results.json" "$disk" >/dev/null 2>&1 || true
debugfs -R "dump /var/lib/winlux/window-latency.json $work/window-latency.json" "$disk" >/dev/null 2>&1 || true
grep -q 'WINLUX_NATIVE_WINDOW_SNAPPING=0' "$log" || { tail -60 "$log"; exit 1; }
for marker in WINLUX_NATIVE_DESKTOP_FILE_SERVICE=0 WINLUX_NATIVE_SETTINGS_ROUTES=0 WINLUX_NATIVE_WINDOW_CONTROLS=0 WINLUX_NATIVE_EDITOR_SAVE=0 WINLUX_NATIVE_EDGE_WINDOWED=0 WINLUX_NATIVE_TASKBAR_AND_MENU=0 WINLUX_NATIVE_PIPES_ANIMATED=0 WINLUX_NATIVE_CLOSE=0 WINLUX_NATIVE_RESULT=0 WINLUX_NATIVE_EXIT=0; do
    if ! grep -q "$marker" "$log"; then
        echo "Missing $marker; see $work" >&2
        tail -60 "$log"
        exit 1
    fi
done
test "$result" = 0 || { tail -60 "$log"; exit 1; }
if test "${WINLUX_KEEP_TEST_DISK:-0}" != 1; then rm "$disk"; fi
echo "OK: mixed PE/Linux windows, controls, editor, catalogue, panel and animated pipes. Captures: $work"
