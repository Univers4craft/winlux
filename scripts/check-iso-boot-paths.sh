#!/bin/sh
# Vérifie qu'une ISO Winlux démarre par les trois chemins :
#  - BIOS (image El Torito et MBR hybride, pour les clés USB) ;
#  - UEFI 64 bits (EFI/BOOT/BOOTX64.EFI) ;
#  - UEFI 32 bits (EFI/BOOT/BOOTIA32.EFI) : processeur 64 bits sur un
#    micrologiciel 32 bits, comme les tablettes Atom. Le système reste 64 bits.
# Le micrologiciel choisit seul le fichier de son mode : les trois coexistent.
#   sh scripts/check-iso-boot-paths.sh ISO
set -eu
iso=${1:?ISO requise}
for tool in xorriso mdir; do
    command -v "$tool" >/dev/null 2>&1 || { echo "outil requis absent : $tool (paquets xorriso, mtools)" >&2; exit 1; }
done
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
report=$(xorriso -indev "$iso" -report_el_torito plain 2>/dev/null)
printf '%s\n' "$report" | grep -q 'El Torito boot img : *1 *BIOS' || { echo "ISO sans amorçage BIOS : $iso" >&2; exit 1; }
printf '%s\n' "$report" | grep -q 'El Torito boot img : *[0-9] *UEFI' || { echo "ISO sans amorçage UEFI : $iso" >&2; exit 1; }
test "$(od -An -tx1 -j510 -N2 "$iso" | tr -d ' ')" = 55aa || { echo "ISO non hybride (clé USB) : $iso" >&2; exit 1; }
xorriso -osirrox on -indev "$iso" -extract_boot_images "$work/" >/dev/null 2>&1
found=
for image in "$work"/*uefi*.img "$work"/gpt_part*efi*.img; do
    test -f "$image" || continue
    files=$(mdir -b -i "$image" ::/EFI/BOOT 2>/dev/null | tr '[:lower:]' '[:upper:]')
    for loader in BOOTX64.EFI BOOTIA32.EFI; do
        printf '%s\n' "$files" | grep -q "/$loader\$" || { echo "chargeur $loader absent de $(basename "$image") : $iso" >&2; exit 1; }
    done
    found=1
done
test -n "$found" || { echo "partition EFI introuvable dans $iso" >&2; exit 1; }
echo "Amorçage BIOS, UEFI 64 bits et UEFI 32 bits présents : $iso"
