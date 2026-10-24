#!/bin/sh
# Real GRUB boot, DHCP/DNS/HTTPS and root persistence across two boots.
# Only use this with a disposable disk; it writes /etc/winlux/smoke-count.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
disk=${1:?usage: test-system-vm.sh disposable.qcow2 [iso]}
iso=${2:-"$root/build/winlux-live-test.iso"}
test -f "$disk" && test -f "$iso"
accel=tcg
if test -r /dev/kvm && test -w /dev/kvm; then accel=kvm; fi
for boot in 1 2; do
    log="$root/build/system-smoke-$boot.log"
    set +e
    (
        for attempt in $(seq 1 80); do printf s || exit 0; sleep 0.5; done
    ) | timeout -k 5s 100 qemu-system-x86_64 -machine q35,accel="$accel" \
        -m 2048 -smp 2 -display none -monitor none -serial stdio -no-reboot \
        -device virtio-vga -device virtio-tablet-pci -device virtio-keyboard-pci \
        -netdev user,id=net -device virtio-net-pci,netdev=net \
        -drive "file=$disk,if=virtio,format=qcow2" -cdrom "$iso" -boot d >"$log" 2>&1
    result=$?
    set -e
    if test "$result" -ne 0; then tail -n 70 "$log"; exit 1; fi
    for marker in WINLUX_ROOT_MODE=ext4 WINLUX_NETWORK_HTTPS_EXIT=0 "WINLUX_PERSISTENCE_COUNT=$boot" persistent=1; do
        grep -q "$marker" "$log" || { echo "Missing $marker; see $log" >&2; tail -n 70 "$log"; exit 1; }
    done
    echo "Persistent system boot $boot: GRUB, DHCP, DNS, HTTPS and disk state OK."
done
