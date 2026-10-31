#!/bin/sh
# Test du véritable installateur sans root sur l'hôte : root seulement en VM.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
iso=${1:-"$root/build/winlux-validation.iso"}
stage="$root/build/winlux-live-test/initramfs-root"
bsdtar=${WINLUX_BSDTAR:-$(command -v bsdtar || true)}
test -x "$bsdtar" || { echo 'bsdtar requis (ou WINLUX_BSDTAR=/chemin/bsdtar)' >&2; exit 1; }
test -r "$iso" && test -f "$stage/init" || { echo 'construisez l’ISO et son stage' >&2; exit 1; }
for tool in fakeroot cpio gzip qemu-system-x86_64 timeout; do command -v "$tool" >/dev/null; done
work=$(mktemp -d "$root/build/install-guest.XXXXXX")
accel=tcg
if test -r /dev/kvm && test -w /dev/kvm; then accel=kvm; fi
echo "Artefacts et journaux du test : $work"
# Les artefacts restent disponibles, y compris sur échec. Aucun disque préexistant.
guest="$work/root"
mkdir "$guest"
cp -a "$stage/." "$guest/"
mkdir -p "$guest/opt/winlux-test/scripts" "$guest/usr/lib/grub" "$guest/usr/share"
cp "$iso" "$guest/opt/winlux-test/source.iso"
cp "$root/scripts/install.sh" "$root/scripts/install-safety.sh" "$guest/opt/winlux-test/scripts/"
cp "$root/scripts/test-install-guest-init.sh" "$guest/install-init"
chmod +x "$guest/install-init"
copy_binary() {
    binary=$1
    name=$2
    # Les applets du stage sont des liens vers BusyBox : ne pas écraser leur cible.
    cp --remove-destination -L "$binary" "$guest/usr/bin/$name"
    if ! dependencies=$(ldd "$binary" 2>&1); then
        case "$dependencies" in
            *'not a dynamic executable'*|*'statically linked'*) dependencies= ;;
            *) echo "$dependencies" >&2; exit 1 ;;
        esac
    fi
    case "$dependencies" in *'not found'*) echo "$binary : dépendance absente" >&2; exit 1 ;; esac
    printf '%s\n' "$dependencies" | awk '$2 == "=>" && $3 ~ /^\// {print $3} $1 ~ /^\// {print $1}' | sort -u |
    while IFS= read -r lib; do
        mkdir -p "$guest$(dirname -- "$lib")"
        cp -L "$lib" "$guest$lib"
    done
}
for tool in readlink lsblk findmnt swapon flock losetup parted partprobe mkfs.ext4 mkfs.fat mount umount grub-install grub-script-check blkid xorriso gzip unsquashfs cp du df; do
    binary=$(command -v "$tool")
    copy_binary "$binary" "$tool"
done
copy_binary "$bsdtar" bsdtar
for binary in /usr/bin/grub-* /usr/sbin/grub-*; do
    test -f "$binary" || continue
    copy_binary "$binary" "$(basename -- "$binary")"
done
cp -a /usr/lib/grub/i386-pc /usr/lib/grub/x86_64-efi "$guest/usr/lib/grub/"
cp -a /usr/share/grub "$guest/usr/share/"
test ! -f /etc/mke2fs.conf || cp /etc/mke2fs.conf "$guest/etc/"
for binary in /usr/lib/grub/i386-pc/grub-bios-setup /usr/lib/grub/x86_64-efi/grub-bios-setup; do
    test ! -f "$binary" || copy_binary "$binary" grub-bios-setup
done
mkfifo "$work/archive.pipe"
fakeroot sh -c '
    set -eu
    cd "$1"
    rm -f dev/console dev/null dev/zero
    mknod -m 600 dev/console c 5 1
    mknod -m 666 dev/null c 1 3
    mknod -m 666 dev/zero c 1 5
    chown -R 0:0 .
    find . -print0 | cpio --null --create --format=newc --quiet
' sh "$guest" >"$work/archive.pipe" &
archive_pid=$!
gzip -1 <"$work/archive.pipe" >"$work/install-initramfs.gz"
wait "$archive_pid"
rm "$work/archive.pipe"
disk="$work/installed.img"
# The APT-native image is about 2 GiB compressed: leave room to install it.
truncate -s 12G "$disk"
log="$work/install.log"
install_status=0
(
    for attempt in $(seq 1 540); do
        if grep -Eq 'WINLUX_INSTALL_EXIT=|Kernel panic' "$log" 2>/dev/null; then exit 0; fi
        if grep -q 'Saisissez exactement' "$log" 2>/dev/null; then
            printf 'EFFACER /dev/vda\n'
            exit 0
        fi
        sleep 1
    done
) | timeout "${WINLUX_INSTALL_TEST_TIMEOUT:-600}" qemu-system-x86_64 -m "${WINLUX_QEMU_MEMORY:-6144}" -smp 2 -accel "$accel" -cpu max -display none -monitor none -serial stdio -nic none -no-reboot \
    -kernel "$stage/boot/vmlinuz" -initrd "$work/install-initramfs.gz" \
    -append 'console=ttyS0,115200 rootfstype=ramfs rdinit=/install-init panic=1 winlux.install-test=1' \
    -drive "file=$disk,format=raw,if=virtio" >"$log" 2>&1 || install_status=$?
# A timeout is a failure too: show where the installer stopped instead of
# letting set -e end the script without a word.
grep -q '^WINLUX_INSTALL_EXIT=0' "$log" || { echo "Installation en VM échouée (code ${install_status:-0}) :" >&2; tail -n 80 "$log"; exit 1; }
for mode in bios uefi; do
    log="$work/$mode.log"
    set -- qemu-system-x86_64 -m 2048 -smp 2 -accel "$accel" -cpu max -display none -monitor none -serial stdio -nic none -no-reboot -drive "file=$disk,format=raw,if=virtio"
    if test "$mode" = uefi; then
        cp "${WINLUX_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}" "$work/vars.fd"
        set -- "$@" -machine q35 -drive "if=pflash,format=raw,unit=0,readonly=on,file=${WINLUX_OVMF:-/usr/share/OVMF/OVMF_CODE_4M.fd}" -drive "if=pflash,format=raw,unit=1,file=$work/vars.fd"
    fi
    boot_status=0
    (for attempt in $(seq 1 80); do printf d || exit 0; sleep 0.5; done) | timeout 300 "$@" >"$log" 2>&1 || boot_status=$?
    for marker in WINLUX_SYSTEM_VERSION= WINLUX_LIVE_SEQUENTIAL_EXIT=0 WINLUX_LIVE_PARALLEL_EXIT=0 WINLUX_LIVE_X11_EXIT=0 WINLUX_LIVE_RESULT=0; do
        grep -q "$marker" "$log" || { echo "Disque installé ($mode) : $marker absent (code $boot_status)" >&2; tail -n 80 "$log"; exit 1; }
    done
    echo "Disque installé validé : $mode"
done
# Retain logs on success, and every diagnostic artefact on failure. The
# disposable root and embedded ISO otherwise consume the runner's disk space
# needed for the release bundle after these checks have passed.
rm -rf -- "$guest"
rm -f "$work/install-initramfs.gz"
if test "${WINLUX_KEEP_TEST_DISK:-0}" != 1; then rm "$disk"; fi
