#!/bin/sh
# Keep the full desktop out of GRUB's low-memory initrd allocation.
set -eu
stage=${1:?stage required}
iso_root=${2:?ISO root required}
kernel_release=${3:?kernel release required}
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
# Binaires du petit initramfs pris dans la racine native ($stage, paquets
# amd64 de la base APT), jamais sur la machine de construction : le runner
# ubuntu-26.04 installe la variante amd64v3 (AVX2), et son busybox plantait
# dès /init sur tout processeur sans AVX (Atom, Celeron, Core 1re gén., #2146).
busybox="$stage/usr/bin/busybox"
test -f "$busybox" || busybox="$stage/bin/busybox"
test -f "$busybox" || { echo "busybox absent de la racine $stage" >&2; exit 1; }
linkage=$(LC_ALL=C ldd "$busybox" 2>&1 || true)
case "$linkage" in *'not a dynamic executable'*|*'statically linked'*) ;;
    *) echo 'busybox-static requis pour le petit initramfs' >&2; exit 1 ;;
esac
for applet in sh mount umount mdev insmod awk seq findfs switch_root cat mkdir chmod sleep chroot sha256sum loadkmap reboot sync mv rm cp grep sed head dd chown; do
    "$busybox" --list | grep -Fxq "$applet" || { echo "applet de démarrage absente : $applet" >&2; exit 1; }
done
work=$(mktemp -d "$project/build/live-boot.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
tiny="$work/root"
mkdir -p "$tiny/bin" "$tiny/dev" "$tiny/etc" "$tiny/proc" "$tiny/run" "$tiny/sys" "$tiny/media" "$tiny/lower" "$tiny/cow" "$tiny/newroot" "$iso_root/boot"
cp -L "$busybox" "$tiny/bin/busybox"
ln -s busybox "$tiny/bin/sh"
cp "$project/scripts/winlux-live-init.sh" "$tiny/init"
cp "$project/scripts/winlux-reset-apply.sh" "$tiny/reset-apply.sh"
chmod 755 "$tiny/init" "$tiny/bin/busybox"
chmod 644 "$tiny/reset-apply.sh"
: >"$tiny/modules-to-load"
for driver in loop isofs squashfs overlay ext4; do
    modprobe -S "$kernel_release" --show-depends "$driver" >/dev/null 2>&1 || {
        echo "pilote requis pour le démarrage absent : $driver ($kernel_release)" >&2
        exit 1
    }
done
crypt_drivers=
# cryptsetup et ses bibliothèques : ceux de la racine native, résolus par son
# propre chargeur (le ldd de l'hôte rendrait les bibliothèques de l'hôte).
cryptsetup=
for candidate in "$stage/usr/sbin/cryptsetup" "$stage/sbin/cryptsetup"; do
    if test -f "$candidate"; then cryptsetup=$candidate; break; fi
done
native_libs="$stage/usr/lib/x86_64-linux-gnu"
loader="$native_libs/ld-linux-x86-64.so.2"
if test -n "$cryptsetup" && test -f "$loader"; then
    # Installed systems with an encrypted root unlock it from this initramfs.
    cp -L "$cryptsetup" "$tiny/bin/cryptsetup"
    chmod 755 "$tiny/bin/cryptsetup"
    # glibc dlopens libgcc_s when the Argon2 threads exit; ldd does not list it.
    test -f "$native_libs/libgcc_s.so.1" || { echo 'libgcc_s.so.1 absente de la racine native (requise par cryptsetup)' >&2; exit 1; }
    { LC_ALL=C "$loader" --library-path "$native_libs" --list "$cryptsetup" |
        awk '$2 == "=>" && $3 ~ /^\// { print $3 } $1 ~ /^\// { print $1 }'
      echo "$native_libs/libgcc_s.so.1"; } | sort -u >"$work/crypt-libs"
    while IFS= read -r lib; do
        case "$lib" in
            "$stage"/*) relative=${lib#"$stage"} ;;
            /lib64/ld-linux-x86-64.so.2) relative=$lib; lib=$loader ;;
            *) echo "cryptsetup : bibliothèque hors de la racine native : $lib" >&2; exit 1 ;;
        esac
        relative=$(printf '%s' "$relative" | sed 's|^/lib/|/usr/lib/|')
        mkdir -p "$tiny$(dirname -- "$relative")"
        cp -L "$lib" "$tiny$relative"
    done <"$work/crypt-libs"
    # Chemin du chargeur inscrit dans les binaires (PT_INTERP).
    mkdir -p "$tiny/lib64"
    cp -L "$loader" "$tiny/lib64/ld-linux-x86-64.so.2"
    crypt_drivers='dm_mod dm_crypt xts cbc aesni_intel aes_generic sha256_generic'
else
    echo 'cryptsetup absent de la racine native : les installations chiffrées (LUKS) ne seront pas proposées' >&2
fi
for driver in virtio_pci virtio_blk virtio_scsi sr_mod sd_mod ata_piix ahci nvme vmd usb_storage uas xhci_pci xhci_hcd ehci_pci ehci_hcd uhci_hcd hid usbhid hid_generic mmc_core mmc_block sdhci sdhci_pci sdhci_acpi loop isofs squashfs overlay vfat ext4 $crypt_drivers; do
    # Pas de pipeline : l'échec de modprobe (pilote absent) est visible et la
    # boucle, lue depuis un fichier, s'exécute dans le shell courant (set -e).
    if ! modprobe -S "$kernel_release" --show-depends "$driver" >"$work/depends" 2>"$work/depends.err"; then
        echo "pilote optionnel absent du noyau $kernel_release : $driver ($(tr '\n' ' ' <"$work/depends.err"))" >&2
        continue
    fi
    while read -r directive module rest; do
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
    done <"$work/depends"
done
# Image reproductible : horodatages figés (SOURCE_DATE_EPOCH, sinon 0) quand
# mksquashfs (≥ 4.4) propose -mkfs-time/-all-time ; sinon l'option est omise.
squash_time_options=
if mksquashfs -help 2>&1 | grep -q -- '-all-time'; then
    squash_time_options="-mkfs-time ${SOURCE_DATE_EPOCH:-0} -all-time ${SOURCE_DATE_EPOCH:-0}"
fi
# Device metadata must be recorded in the same fakeroot session as packing.
# xz keeps the ISO under GitHub's 2 GiB per-file release limit (gzip was
# 2.04 GiB at v0.2.10); the Ubuntu kernel's squashfs reads xz natively.
fakeroot sh -c '
    set -eu
    rm -f "$1/dev/console" "$1/dev/null" "$1/dev/zero" "$1/dev/tty" "$1/dev/ttyS0"
    mknod -m 600 "$1/dev/console" c 5 1
    mknod -m 666 "$1/dev/null" c 1 3
    mknod -m 666 "$1/dev/zero" c 1 5
    mknod -m 666 "$1/dev/tty" c 5 0
    mknod -m 666 "$1/dev/ttyS0" c 4 64
    # shellcheck disable=SC2086 # $3 : options facultatives séparées par des espaces
    mksquashfs "$1" "$2" -noappend -all-root -processors "$(nproc)" -comp xz -Xbcj x86 -Xdict-size 100% -b 1M -no-progress $3
' sh "$stage" "$iso_root/boot/winlux-rootfs.squashfs" "$squash_time_options"
# Garde-fou #2146 : aucun binaire de l'initramfs ne doit exiger AVX.
sh "$project/scripts/check-initramfs-baseline.sh" "$tiny"
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
