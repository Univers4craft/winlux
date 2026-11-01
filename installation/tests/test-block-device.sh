#!/bin/sh
# Test réel du créateur de clé sur un périphérique bloc (à lancer en root) :
#  1. une « clé » loop de 64 Mio avec une partition ext4 montée et un fichier ;
#  2. écriture d'une ISO hybride fabriquée par grub-mkrescue, comme celle de
#     Winlux, avec la ligne de commande de l'outil ;
#  3. contrôles : partition démontée par l'outil, copie identique octet pour octet ;
#  4. démarrage de la clé par QEMU, branchée en USB, en BIOS puis en UEFI.
# Usage : sudo installation/tests/test-block-device.sh [ISO]
# Avec ISO (par ex. la vraie ISO Winlux), la clé fait la taille de l'ISO + 64 Mio.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
tool="$here/../winlux-usb-creator.py"
python=${PYTHON:-python3}
test "$(id -u)" = 0 || { echo "à lancer en root" >&2; exit 1; }
work=$(mktemp -d)
loop=""
cleanup() {
    for pid in $(cat "$work"/*.pid 2>/dev/null); do kill "$pid" 2>/dev/null || true; done
    umount "$work/mnt" 2>/dev/null || true
    test -z "$loop" || losetup -d "$loop" 2>/dev/null || true
    rm -rf "$work"
}
trap cleanup EXIT

iso=${1:-}
if test -z "$iso"; then
    iso="$work/test.iso"
    mkdir -p "$work/iso/boot/grub"
    cat >"$work/iso/boot/grub/grub.cfg" <<'GRUB'
serial --unit=0 --speed=115200
terminal_input console serial
terminal_output console serial
echo "WINLUX-USB-BOOT-OK-${grub_platform}"
sleep 600
GRUB
    grub-mkrescue -o "$iso" "$work/iso" >/dev/null 2>&1
fi
iso_size=$(stat -c %s "$iso")

truncate -s $((iso_size + 64 * 1024 * 1024)) "$work/stick.img"
loop=$(losetup -fP --show "$work/stick.img")
printf 'label: dos\n,,83\n' | sfdisk -q "$loop"
partx -u "$loop" 2>/dev/null || true
udevadm settle 2>/dev/null || true
mkfs.ext4 -q -F "${loop}p1"
mkdir -p "$work/mnt"
mount "${loop}p1" "$work/mnt"
echo "ancien contenu de la clé" >"$work/mnt/ancien.txt"

echo "== Liste des disques (loop visible pour le test)"
WINLUX_USB_INCLUDE_LOOPS=1 "$python" "$tool" list --all
echo "== Un disque non amovible est refusé sans --allow-fixed"
if WINLUX_USB_INCLUDE_LOOPS=1 "$python" "$tool" write --image "$iso" --device "$loop" --yes; then
    echo "ÉCHEC : écriture acceptée sur un disque non amovible" >&2; exit 1
fi
mountpoint -q "$work/mnt" || { echo "ÉCHEC : refus mais clé démontée" >&2; exit 1; }

echo "== Écriture de l'ISO sur $loop"
WINLUX_USB_INCLUDE_LOOPS=1 "$python" "$tool" write --image "$iso" --device "$loop" --yes --allow-fixed | tail -n 2
if mountpoint -q "$work/mnt"; then echo "ÉCHEC : partition toujours montée" >&2; exit 1; fi
cmp -n "$iso_size" "$iso" "$loop"
echo "Copie identique ($iso_size octets)."
losetup -d "$loop"
loop=""

if test -n "${1:-}" || ! command -v qemu-system-x86_64 >/dev/null; then
    echo "Démarrage QEMU non testé (ISO fournie ou QEMU absent)."
    exit 0
fi
accel=tcg
test -w /dev/kvm && accel=kvm
ovmf=""
for f in /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd /usr/share/ovmf/OVMF.fd; do
    if test -f "$f"; then ovmf=$f; break; fi
done
for mode in bios uefi; do
    fw=""
    if test "$mode" = uefi; then
        test -n "$ovmf" || { echo "OVMF absent : UEFI non testé" >&2; exit 1; }
        vars=$(dirname "$ovmf")/OVMF_VARS_4M.fd
        test -f "$vars" || vars=$(dirname "$ovmf")/OVMF_VARS.fd
        if test -f "$vars"; then
            cp "$vars" "$work/vars.fd"
            fw="-drive if=pflash,format=raw,readonly=on,file=$ovmf -drive if=pflash,format=raw,file=$work/vars.fd"
        else
            fw="-bios $ovmf"
        fi
    fi
    : >"$work/serial-$mode.log"
    # shellcheck disable=SC2086
    qemu-system-x86_64 -machine q35 -accel "$accel" -m 512 $fw -display none -serial "file:$work/serial-$mode.log" \
        -device qemu-xhci -drive if=none,id=stick,format=raw,file="$work/stick.img",snapshot=on \
        -device usb-storage,drive=stick,bootindex=0 -no-reboot &
    echo $! >"$work/qemu-$mode.pid"
    ok=0
    for _ in $(seq 1 180); do
        if grep -aq "WINLUX-USB-BOOT-OK" "$work/serial-$mode.log"; then ok=1; break; fi
        sleep 1
    done
    kill "$(cat "$work/qemu-$mode.pid")" 2>/dev/null || true
    rm -f "$work/qemu-$mode.pid"
    if test "$ok" = 1; then
        echo "Démarrage depuis la clé USB en $mode : OK ($(grep -ao 'WINLUX-USB-BOOT-OK-[a-z0-9-]*' "$work/serial-$mode.log" | head -n 1))"
    else
        echo "ÉCHEC : la clé ne démarre pas en $mode" >&2
        tail -c 2000 "$work/serial-$mode.log" >&2
        exit 1
    fi
done
echo "Tous les tests sur périphérique bloc sont passés."
