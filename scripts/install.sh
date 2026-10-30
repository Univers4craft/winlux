#!/bin/sh
# Installateur expérimental : effacement d'un disque entier, jamais d'une partition.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$root/scripts/install-safety.sh"
die() { install_die "$@"; }
usage() {
    echo "Usage: sudo scripts/install.sh DEVICE --iso IMAGE [--check-only] [--test-image RAW]"
    echo "Efface le disque entier après saisie exacte : EFFACER /dev/..."
    echo "--check-only : contrôles sans effacement ; --test-image : automatisation limitée à un loop adossé à ce fichier."
}
device= iso= test_image= check_only=0
while test $# -gt 0; do
    case "$1" in
        --iso|--test-image)
            test $# -ge 2 || die "valeur manquante pour $1"
            if test "$1" = --iso; then iso=$2; else test_image=$2; fi
            shift 2 ;;
        --check-only) check_only=1; shift ;;
        --no-part|--label) die "option retirée : seuls les disques entiers avec label WINLUX_ROOT sont pris en charge" ;;
        --help|-h) usage; exit 0 ;;
        -*) die "option inconnue : $1" ;;
        *) test -z "$device" || die 'une seule cible est autorisée'; device=$1; shift ;;
    esac
done
test -n "$device" || { usage >&2; exit 1; }
device=$(readlink -e -- "$device") || die 'cible introuvable'
test "$(id -u)" -eq 0 || die 'droits root requis'
iso=${iso:-"$root/build/winlux-live-test.iso"}
iso=$(readlink -e -- "$iso") || die 'ISO introuvable'
test -f "$iso" && test -r "$iso" || die 'ISO non lisible'
for tool in lsblk findmnt swapon flock losetup parted partprobe mkfs.ext4 mkfs.fat mount umount grub-install grub-script-check blkid xorriso bsdtar gzip cp; do
    command -v "$tool" >/dev/null 2>&1 || die "outil requis absent : $tool"
done
test -d /usr/lib/grub/i386-pc && test -d /usr/lib/grub/x86_64-efi || die 'modules GRUB BIOS et UEFI requis'
install_check_device "$device"
identity=$(lsblk -bdnro MAJ:MIN,SIZE,MODEL,SERIAL "$device")
# Aucune option --yes générique : les tests automatisés sont limités aux fichiers loop.
if test -n "$test_image"; then
    test_image=$(readlink -e -- "$test_image") || die 'image de test introuvable'
    test -f "$test_image" || die 'image de test non régulière'
    test "$(lsblk -dnro TYPE "$device")" = loop || die '--test-image exige un périphérique loop'
    backing=$(losetup -n -O BACK-FILE --raw "$device")
    test "$(readlink -e -- "$backing")" = "$test_image" || die 'le loop ne correspond pas au fichier de test'
    test "$(losetup -j "$test_image" | wc -l)" -eq 1 || die 'image déjà attachée à plusieurs loop'
fi
work=$(mktemp -d /tmp/winlux-install.XXXXXX)
mounted=0
esp_mounted=0
cleanup() {
    # Ne jamais effacer un répertoire encore monté, même après erreur.
    if test "$esp_mounted" = 1; then umount "$work/target/boot/efi" || return; fi
    if test "$mounted" = 1; then umount "$work/target" || return; fi
    rm -rf -- "$work"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
mkdir "$work/source" "$work/target"
iso_sha=$(sha256sum "$iso" | awk '{print $1}')
xorriso -osirrox on -indev "$iso" -extract /boot/vmlinuz "$work/vmlinuz" >/dev/null || die 'extraction du noyau impossible'
xorriso -osirrox on -indev "$iso" -extract /boot/winlux-initramfs.cpio.gz "$work/initramfs.gz" >/dev/null || die 'extraction de l’initramfs impossible'
test -s "$work/vmlinuz" && test -s "$work/initramfs.gz" || die 'ISO incomplète'
gzip -t "$work/initramfs.gz"
root_extent=$(xorriso -indev "$iso" -find /boot/winlux-rootfs.squashfs -exec report_lba -- 2>/dev/null |
    awk -F, '/^File data lba:/ {gsub(/[[:space:]]/, "", $2); print $2}')
split_root=0
# bsdtar refuse par défaut les chemins absolus, traversées .. et parents symlink.
# Déployer uniquement une ISO Winlux de confiance ; aucune signature n'est encore disponible.
if test -n "$root_extent"; then
    case "$root_extent" in *[!0-9]*) die 'étendue SquashFS invalide ou fragmentée' ;; esac
    test "${#root_extent}" -le 12 || die 'étendue SquashFS excessive'
    command -v unsquashfs >/dev/null 2>&1 || die 'squashfs-tools requis pour cette ISO'
    # Read the filesystem directly from its ISO extent; no second compressed
    # copy in /tmp, whose capacity is deliberately limited in the live session.
    rmdir "$work/source"
    unsquashfs -no-progress -processors 2 -offset "$((root_extent * 2048))" -d "$work/source" "$iso"
    split_root=1
else
    bsdtar -xf "$work/initramfs.gz" -C "$work/source" --no-same-owner
fi
for path in init usr/bin/winlux usr/share/winlux/BUILD-INFO; do
    test -f "$work/source/$path" && test ! -L "$work/source/$path" || die "racine ISO incomplète : $path"
done
test -x "$work/source/init" || die 'init non exécutable'
test -c "$work/source/dev/console" || die 'console de démarrage absente de la racine'
test "$(sha256sum "$iso" | awk '{print $1}')" = "$iso_sha" || die 'ISO modifiée pendant la préparation'
needed=$(du -sk "$work/source" | awk '{ print $1 + 262144 }')
budget=$(lsblk -bdnro SIZE "$device" | awk '{ printf "%.0f", ($1 / 1024 - 527360) * 0.94 }')
test "$budget" -gt "$needed" || die 'disque trop petit pour la racine complète'
echo "ATTENTION : toutes les données du disque suivant seront effacées."
lsblk -bd -o NAME,SIZE,MODEL,SERIAL,TRAN "$device"
echo "ISO : $iso"
sha256sum "$iso"
if test "$check_only" = 1; then echo 'Prévalidation réussie ; aucune écriture sur le disque.'; exit 0; fi
if test -z "$test_image"; then
    test -t 0 && test -r /dev/tty || die 'confirmation interactive requise (pas de redirection)'
    printf 'Saisissez exactement « EFFACER %s » : ' "$device" >/dev/tty
    IFS= read -r answer </dev/tty || die 'confirmation annulée'
    install_confirmation_matches "$answer" "$device" || die 'confirmation incorrecte : installation annulée'
fi
# Revalider juste avant la première opération destructive.
install_check_device "$device"
test "$(lsblk -bdnro MAJ:MIN,SIZE,MODEL,SERIAL "$device")" = "$identity" || die 'identité du disque modifiée'
lock_dir=/run/winlux-installer
test ! -L "$lock_dir" || die 'répertoire de verrouillage invalide'
mkdir -p -m 700 "$lock_dir"
test "$(stat -c %u "$lock_dir")" = 0 || die 'répertoire de verrouillage non privilégié'
chmod 700 "$lock_dir"
disk_id=$(lsblk -dnro MAJ:MIN "$device")
case "$disk_id" in ''|*[!0-9:]*) die 'identifiant de verrou invalide' ;; esac
# Ne pas verrouiller le périphérique lui-même : parted/udev peuvent le verrouiller.
exec 9>"$lock_dir/$disk_id.lock"
flock -n 9 || die 'disque verrouillé par une autre opération'
parted -s "$device" -- mklabel gpt
parted -s "$device" -- mkpart BIOS 1MiB 3MiB set 1 bios_grub on
parted -s "$device" -- mkpart ESP fat32 3MiB 515MiB set 2 esp on
parted -s "$device" -- mkpart WINLUX_ROOT ext4 515MiB 100%
partprobe "$device"
partition() {
    for attempt in 1 2 3 4 5 6 7 8 9 10; do
        for candidate in "${device}$1" "${device}p$1"; do
            if test -b "$candidate"; then printf '%s\n' "$candidate"; return 0; fi
        done
        sleep 0.2
    done
    return 1
}
esp=$(partition 2) || die 'ESP introuvable'
target=$(partition 3) || die 'partition racine introuvable'
mkfs.fat -F32 "$esp"
mkfs.ext4 -F -L WINLUX_ROOT "$target"
mount "$target" "$work/target"
mounted=1
free=$(df -Pk "$work/target" | awk 'NR==2 { print $4 }')
test "$free" -gt "$needed" || die 'espace insuffisant pour la racine complète'
cp -a "$work/source/." "$work/target/"
mkdir -p "$work/target/boot/grub" "$work/target/boot/efi"
cp "$work/vmlinuz" "$work/target/boot/vmlinuz"
cp "$work/initramfs.gz" "$work/target/boot/winlux-initramfs.cpio.gz"
partuuid=$(blkid -s PARTUUID -o value "$target")
case "$partuuid" in ''|*[!a-fA-F0-9-]*) die 'PARTUUID invalide' ;; esac
fsuuid=$(blkid -s UUID -o value "$target")
case "$fsuuid" in ''|*[!a-fA-F0-9-]*) die 'UUID ext4 invalide' ;; esac
root_argument="PARTUUID=$partuuid"
initrd_line=
if test "$split_root" = 1; then
    root_argument="UUID=$fsuuid"
    initrd_line='initrd /boot/winlux-initramfs.cpio.gz'
    printf '1\n' >"$work/target/boot/winlux-split-root"
fi
cat >"$work/target/boot/grub/grub.cfg" <<EOF
set default=0
set timeout=3
serial --unit=0 --speed=115200
terminal_input console serial
terminal_output console serial
menuentry "Winlux Desktop installé" {
    search --no-floppy --fs-uuid $fsuuid --set=root
    linux /boot/vmlinuz root=$root_argument rw rootwait rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 winlux.interactive=1
    $initrd_line
}
menuentry "Winlux diagnostic installé" --hotkey=d {
    search --no-floppy --fs-uuid $fsuuid --set=root
    linux /boot/vmlinuz root=$root_argument rw rootwait rootfstype=ext4 init=/init console=ttyS0,115200n8
    $initrd_line
}
EOF
grub-script-check "$work/target/boot/grub/grub.cfg"
grub-install --target=i386-pc --boot-directory="$work/target/boot" --recheck "$device"
# Les noms EFI sont ASCII ; éviter de dépendre du module NLS iso8859-1 de l'hôte.
mount -t vfat -o iocharset=ascii "$esp" "$work/target/boot/efi"
esp_mounted=1
grub-install --target=x86_64-efi --efi-directory="$work/target/boot/efi" --boot-directory="$work/target/boot" --removable --no-nvram --recheck
sync
umount "$work/target/boot/efi"
esp_mounted=0
umount "$work/target"
mounted=0
echo "Installation complète sur $device."
