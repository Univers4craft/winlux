#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
mode=${1:-run}
case "$mode" in
    run|--create-only) ;;
    *) echo "usage: $0 [--create-only]" >&2; exit 2 ;;
esac
vm_dir=${WINLUX_VM_DIR:-"$root/build/vm"}
disk=${WINLUX_VM_DISK:-"$vm_dir/winlux.qcow2"}
disk_size=${WINLUX_VM_DISK_SIZE:-24G}
memory=${WINLUX_VM_MEMORY:-4096}
cpus=${WINLUX_VM_CPUS:-4}
width=${WINLUX_VM_WIDTH:-1920}
height=${WINLUX_VM_HEIGHT:-1080}
iso=${WINLUX_VM_ISO:-"$root/build/winlux-live-test.iso"}
auto_stop=${WINLUX_VM_AUTO_STOP_SECONDS:-60}
monitor="$vm_dir/monitor.sock"

command -v qemu-system-x86_64 >/dev/null 2>&1 || { echo "qemu-system-x86_64 est requis" >&2; exit 1; }
command -v qemu-img >/dev/null 2>&1 || { echo "qemu-img est requis" >&2; exit 1; }
command -v timeout >/dev/null 2>&1 || { echo "timeout est requis" >&2; exit 1; }
test -f "$iso" || { echo "ISO absente : $iso" >&2; exit 1; }
kernel="$root/build/winlux-live-test/iso-root/boot/vmlinuz"
stage="$root/build/winlux-live-test/initramfs-root"
test -f "$kernel" || { echo "noyau VM absent; reconstruisez l'ISO" >&2; exit 1; }
test -x "$stage/init" || { echo "système racine absent; reconstruisez l'ISO" >&2; exit 1; }

mkdir -p "$vm_dir"
if ! test -f "$disk"; then
    command -v mkfs.ext4 >/dev/null 2>&1 || { echo "mkfs.ext4 est requis" >&2; exit 1; }
    raw="$vm_dir/winlux-root.raw"
    truncate -s "$disk_size" "$raw"
    mkfs.ext4 -q -F -L WINLUX_ROOT -d "$stage" "$raw"
    qemu-img convert -f raw -O qcow2 "$raw" "$disk"
    rm -f "$raw"
fi

accel=tcg
if test -r /dev/kvm && test -w /dev/kvm; then accel=kvm; fi
cpu=max
if test "$accel" = kvm; then cpu=host; fi
echo "VM Winlux prête : $disk"
if test "$mode" = --create-only; then
    echo "Disque persistant créé sans démarrer QEMU."
    exit 0
fi
case "$auto_stop" in *[!0-9]*|'') echo "WINLUX_VM_AUTO_STOP_SECONDS doit être un entier positif" >&2; exit 2 ;; esac
echo "Démarrage de la préversion graphique avec $memory Mio, $cpus CPU, accélération $accel. Arrêt forcé dans $auto_stop secondes."
rm -f "$monitor"
# A virtio tablet provides absolute pointer coordinates without capturing the
# host's mouse.  The guest image includes virtio-input; its USB HID stack is
# not guaranteed to enumerate an emulated USB tablet.
# xres/yres only set the initial mode: resizing the QEMU window announces a
# new preferred mode to the guest, which applies it with xrandr.
exec timeout --foreground --signal=TERM -k 5s "$auto_stop" qemu-system-x86_64 \
    -name "Winlux Desktop Preview" \
    -machine q35,accel="$accel" \
    -cpu "$cpu" \
    -m "$memory" \
    -smp "$cpus" \
    -device "virtio-vga,xres=$width,yres=$height" \
    -display gtk,full-screen=on,zoom-to-fit=on \
    -device virtio-tablet-pci \
    -device virtio-keyboard-pci \
    -monitor "unix:$monitor,server=on,wait=off" \
    -drive "file=$disk,if=virtio,format=qcow2,cache=writeback" \
    -drive "file=$iso,media=cdrom,readonly=on" \
    -kernel "$kernel" \
    -append "root=/dev/vda rw rootfstype=ext4 init=/init panic=1 loglevel=3 winlux.interactive=1 console=tty1 console=ttyS0,115200n8" \
    -serial "file:$vm_dir/serial.log"
