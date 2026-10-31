#!/bin/sh
# Keep the full desktop out of GRUB's low-memory initrd allocation.
set -eu
stage=${1:?stage required}
iso_root=${2:?ISO root required}
kernel_release=${3:?kernel release required}
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
linkage=$(LC_ALL=C ldd /usr/bin/busybox 2>&1 || true)
case "$linkage" in *'not a dynamic executable'*|*'statically linked'*) ;;
    *) echo 'busybox-static requis pour le petit initramfs' >&2; exit 1 ;;
esac
for applet in sh mount umount mdev insmod awk seq findfs switch_root cat mkdir chmod sleep; do
    /usr/bin/busybox --list | grep -Fxq "$applet" || { echo "applet de démarrage absente : $applet" >&2; exit 1; }
done
work=$(mktemp -d "$project/build/live-boot.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
tiny="$work/root"
mkdir -p "$tiny/bin" "$tiny/dev" "$tiny/etc" "$tiny/proc" "$tiny/run" "$tiny/sys" "$tiny/media" "$tiny/lower" "$tiny/cow" "$tiny/newroot" "$iso_root/boot"
cp /usr/bin/busybox "$tiny/bin/busybox"
ln -s busybox "$tiny/bin/sh"
cp "$project/scripts/winlux-live-init.sh" "$tiny/init"
chmod 755 "$tiny/init" "$tiny/bin/busybox"
: >"$tiny/modules-to-load"
for driver in loop isofs squashfs overlay ext4; do
    modprobe -S "$kernel_release" --show-depends "$driver" >/dev/null 2>&1 || {
        echo "pilote requis pour le démarrage absent : $driver ($kernel_release)" >&2
        exit 1
    }
done
crypt_drivers=
cryptsetup=$(command -v cryptsetup 2>/dev/null || true)
if test -n "$cryptsetup"; then
    # Installed systems with an encrypted root unlock it from this initramfs.
    cp -L "$cryptsetup" "$tiny/bin/cryptsetup"
    chmod 755 "$tiny/bin/cryptsetup"
    LC_ALL=C ldd "$cryptsetup" | awk '$2 == "=>" && $3 ~ /^\// { print $3 } $1 ~ /^\// { print $1 }' | sort -u |
    while IFS= read -r lib; do
        mkdir -p "$tiny$(dirname -- "$lib")"
        cp -L "$lib" "$tiny$lib"
    done
    # glibc dlopens libgcc_s when the Argon2 threads exit; ldd does not list it.
    libgcc=$(ldconfig -p | awk '/libgcc_s\.so\.1 .*x86-64/ { print $NF; exit }')
    test -n "$libgcc" || { echo 'libgcc_s.so.1 introuvable (requis par cryptsetup)' >&2; exit 1; }
    mkdir -p "$tiny$(dirname -- "$libgcc")"
    cp -L "$libgcc" "$tiny$libgcc"
    crypt_drivers='dm_mod dm_crypt xts cbc aesni_intel aes_generic sha256_generic'
else
    echo 'cryptsetup absent : les installations chiffrées (LUKS) ne seront pas proposées' >&2
fi
for driver in virtio_pci virtio_blk virtio_scsi sr_mod sd_mod ata_piix ahci nvme usb_storage uas xhci_pci ehci_pci uhci_hcd hid usbhid hid_generic loop isofs squashfs overlay vfat ext4 $crypt_drivers; do
    modprobe -S "$kernel_release" --show-depends "$driver" 2>/dev/null | while read -r directive module rest; do
        test "$directive" = insmod || continue
        relative=${module#/}
        case "$relative" in *.zst|*.xz|*.gz) relative=${relative%.*} ;; esac
        if ! grep -Fxq "$relative" "$tiny/modules-to-load"; then
            mkdir -p "$tiny/$(dirname "$relative")"
            case "$module" in
                *.zst) zstd -qdc "$module" >"$tiny/$relative" ;;
                *.xz) xz -dc "$module" >"$tiny/$relative" ;;
                *.gz) gzip -dc "$module" >"$tiny/$relative" ;;
                *) cp "$module" "$tiny/$relative" ;;
            esac
            printf '%s\n' "$relative" >>"$tiny/modules-to-load"
        fi
    done
done
# Device metadata must be recorded in the same fakeroot session as packing.
fakeroot sh -c '
    set -eu
    rm -f "$1/dev/console" "$1/dev/null" "$1/dev/zero" "$1/dev/tty" "$1/dev/ttyS0"
    mknod -m 600 "$1/dev/console" c 5 1
    mknod -m 666 "$1/dev/null" c 1 3
    mknod -m 666 "$1/dev/zero" c 1 5
    mknod -m 666 "$1/dev/tty" c 5 0
    mknod -m 666 "$1/dev/ttyS0" c 4 64
    mksquashfs "$1" "$2" -noappend -all-root -processors 2 -comp gzip -b 1M -no-progress
' sh "$stage" "$iso_root/boot/winlux-rootfs.squashfs"
fakeroot sh -c '
    set -eu
    cd "$1"
    mknod -m 600 dev/console c 5 1
    mknod -m 666 dev/null c 1 3
    chown -R 0:0 .
    find . -print0 | cpio --null --create --format=newc --quiet
' sh "$tiny" >"$work/boot.cpio"
gzip -n -9 -c "$work/boot.cpio" >"$iso_root/boot/winlux-initramfs.cpio.gz"
test "$(wc -c <"$iso_root/boot/winlux-initramfs.cpio.gz")" -le 134217728 || { echo 'initramfs de démarrage anormalement volumineux' >&2; exit 1; }
echo "Live root: $(du -h "$iso_root/boot/winlux-rootfs.squashfs")"
echo "Boot initramfs: $(du -h "$iso_root/boot/winlux-initramfs.cpio.gz")"
