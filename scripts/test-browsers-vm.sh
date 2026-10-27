#!/bin/sh
# Only creates and boots its own disposable disk; never opens a user VM disk.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
stage="$root/build/winlux-live-test/initramfs-root"
test -x "$stage/init" && test -s "$stage/boot/vmlinuz"
work=$(mktemp -d "$root/build/browser-vm.XXXXXX")
disk="$work/test.raw"
log="$work/serial.log"
trap 'rm -f "$disk"' EXIT HUP INT TERM
accel=tcg
if test -r /dev/kvm && test -w /dev/kvm; then accel=kvm; fi
truncate -s 8G "$disk"
mkfs.ext4 -q -F -L WINLUX_TEST -d "$stage" "$disk"
echo "Diagnostic navigateurs ($accel), journal : $log"
result=0
timeout -k 5s "${WINLUX_BROWSER_TEST_TIMEOUT:-600}" qemu-system-x86_64 \
    -m "${WINLUX_QEMU_MEMORY:-4096}" -smp 2 -accel "$accel" \
    -display none -monitor none -serial stdio -no-reboot -nic none \
    -kernel "$stage/boot/vmlinuz" \
    -append 'root=/dev/vda rw rootfstype=ext4 init=/init console=ttyS0 panic=1 loglevel=3 winlux.browser-smoke=1' \
    -drive "file=$disk,if=virtio,format=raw" > "$log" 2>&1 || result=$?
if test "$result" != 0; then tail -50 "$log"; exit 1; fi
for browser in edge chrome firefox; do
    grep -q "WINLUX_BROWSER_${browser}_RESULT=0" "$log" || { tail -50 "$log"; exit 1; }
done
grep -q 'WINLUX_BROWSERS_RESULT=0' "$log"
echo 'OK : rendu réel Edge, Chrome et Firefox avec sandbox, installation hors ligne.'
