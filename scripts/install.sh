#!/bin/sh
# Installateur autonome Winlux pour disque réel (BIOS/UEFI).
#
# Installe l'ISO live Winlux sur une partition ext4 avec un bootloader GRUB
# capable de démarrer en BIOS et UEFI. Usage :
#
#   sudo scripts/install.sh /dev/sdX            # disque entier
#   sudo scripts/install.sh /dev/sdX2 --no-part # partition existante
#
# Le script partitionne le disque (sauf --no-part), formate en ext4, monte,
# copie le noyau et l'initramfs depuis l'ISO, installe GRUB, et configure
# le démarrage automatique du bureau Winlux.
set -eu

usage() {
    cat >&2 <<'EOF'
Usage : sudo scripts/install.sh DEVICE [OPTIONS]

DEVICE  : /dev/sdX (disque entier) ou /dev/sdXN (partition, avec --no-part)

Options :
  --no-part       Ne pas partitionner, utiliser le device tel quel
  --iso PATH      ISO source (par défaut : build/winlux-live-test.iso)
  --label LABEL   Étiquette du volume (défaut : WINLUX_ROOT)
  --help          Ce message
EOF
    exit 1
}

die() { echo "ERREUR : $*" >&2; exit 1; }

# --- Arguments ---
device=""
iso=""
label="WINLUX_ROOT"
no_part=0

while test $# -gt 0; do
    case "$1" in
        --no-part) no_part=1; shift ;;
        --iso) iso="$2"; shift 2 ;;
        --label) label="$2"; shift 2 ;;
        --help|-h) usage ;;
        -*) die "option inconnue : $1" ;;
        *)
            if test -z "$device"; then device="$1"; else die "argument inattendu : $1"; fi
            shift ;;
    esac
done

test -n "$device" || usage
test -b "$device" || die "$device n'est pas un périphérique bloc"
test "$(id -u)" -eq 0 || die "ce script nécessite les droits root (sudo)"

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

# Trouver l'ISO.
if test -z "$iso"; then
    iso="$root/build/winlux-live-test.iso"
fi
test -r "$iso" || die "ISO introuvable : $iso (lancez d'abord scripts/build-live-iso.sh)"

for tool in parted mkfs.ext4 mount umount grub-install lsblk; do
    command -v "$tool" >/dev/null 2>&1 || die "outil requis absent : $tool"
done

# --- Vérification de sécurité ---
mounted=$(lsblk -no MOUNTPOINT "$device" 2>/dev/null | grep -v '^$' || true)
if test -n "$mounted"; then
    die "$device a des partitions montées :\n$mounted\nDémontez-les d'abord."
fi

# --- Partitionnement ---
target_part="$device"
if test "$no_part" -eq 0; then
    echo "Partitionnement de $device (GPT + une partition ext4)…"
    parted -s "$device" -- mklabel gpt
    # Partition EFI System (ESP) de 512 Mo.
    parted -s "$device" -- mkpart ESP fat32 1MiB 513MiB set 1 esp on
    # Partition racine Winlux.
    parted -s "$device" -- mkpart "$label" ext4 513MiB 100%
    partprobe "$device" 2>/dev/null || sleep 2

    # Trouver les noms de partition (sda1/nvme0n1p1…).
    esp_part=""
    root_part=""
    for p in "${device}"1 "${device}p1"; do
        test -b "$p" && esp_part="$p" && break
    done
    for p in "${device}"2 "${device}p2"; do
        test -b "$p" && root_part="$p" && break
    done
    test -n "$esp_part" || die "partition ESP introuvable après partitionnement"
    test -n "$root_part" || die "partition racine introuvable après partitionnement"

    echo "Formatage de l'ESP ($esp_part)…"
    mkfs.fat -F32 "$esp_part"
    echo "Formatage de la partition racine ($root_part)…"
    mkfs.ext4 -F -L "$label" "$root_part"
    target_part="$root_part"
    target_esp="$esp_part"
else
    echo "Formatage de $target_part en ext4…"
    mkfs.ext4 -F -L "$label" "$target_part"
    target_esp=""
fi

# --- Montage et copie ---
mountpoint=$(mktemp -d /tmp/winlux-install.XXXXXX)
trap 'umount -R "$mountpoint" 2>/dev/null || true; rmdir "$mountpoint" 2>/dev/null || true' EXIT

mount "$target_part" "$mountpoint"
mkdir -p "$mountpoint/boot"

# Monter l'ISO pour extraire noyau + initramfs.
iso_mount=$(mktemp -d /tmp/winlux-iso.XXXXXX)
mount -o loop,ro "$iso" "$iso_mount"
cp "$iso_mount/boot/vmlinuz" "$mountpoint/boot/vmlinuz"
cp "$iso_mount/boot/winlux-initramfs.cpio.gz" "$mountpoint/boot/winlux-initramfs.cpio.gz"

# Copier le grub.cfg de l'ISO comme base.
mkdir -p "$mountpoint/boot/grub"
cp "$iso_mount/boot/grub/grub.cfg" "$mountpoint/boot/grub/grub.cfg"
umount "$iso_mount"
rmdir "$iso_mount"

# --- GRUB BIOS ---
echo "Installation de GRUB (BIOS i386-pc)…"
if test -d /usr/lib/grub/i386-pc; then
    grub-install --target=i386-pc --boot-directory="$mountpoint/boot" "$device" 2>/dev/null || \
        echo "AVERTISSEMENT : GRUB BIOS non installé (normal si le firmware est UEFI uniquement)"
fi

# --- GRUB UEFI ---
if test -n "$target_esp"; then
    mkdir -p "$mountpoint/boot/efi"
    mount "$target_esp" "$mountpoint/boot/efi"
    echo "Installation de GRUB (UEFI x86_64-efi)…"
    if test -d /usr/lib/grub/x86_64-efi; then
        grub-install --target=x86_64-efi --efi-directory="$mountpoint/boot/efi" \
            --boot-directory="$mountpoint/boot" --removable 2>/dev/null || \
            echo "AVERTISSEMENT : GRUB UEFI non installé"
    fi
fi

# --- Espace persistant ---
mkdir -p "$mountpoint/etc/winlux" "$mountpoint/root"
if test -r "$root/integration/x-ms-dos-executable.xml"; then
    mkdir -p "$mountpoint/usr/share/mime/packages"
    cp "$root/integration/x-ms-dos-executable.xml" "$mountpoint/usr/share/mime/packages/winlux.xml"
fi

echo "Installation terminée sur $device"
echo "  Partition racine : $target_part (label $label)"
test -n "$target_esp" && echo "  Partition ESP     : $target_esp"
echo ""
echo "Redémarrez sur ce disque pour lancer Winlux Desktop."
