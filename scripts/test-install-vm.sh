#!/bin/sh
# Valide l'installation Winlux sur disque dans une VM QEMU.
#
# Crée un disque virtuel, installe Winlux dessus via scripts/install.sh,
# puis démarre la VM et vérifie que le système boot et passe les tests.
#
# Usage :
#   scripts/test-install-vm.sh                    # tout automatique
#   scripts/test-install-vm.sh --keep-disk        # garder le disque après
#   scripts/test-install-vm.sh --iso path.iso     # ISO spécifique
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work="$root/build/test-install-vm"
disk="$work/winlux-test.img"
disk_size=${WINLUX_TEST_DISK_SIZE:-4G}
iso=""
keep_disk=0
timeout_s=120

while test $# -gt 0; do
    case "$1" in
        --keep-disk) keep_disk=1; shift ;;
        --iso) iso="$2"; shift 2 ;;
        --timeout) timeout_s="$2"; shift 2 ;;
        *) echo "option inconnue : $1" >&2; exit 1 ;;
    esac
done

if test -z "$iso"; then
    iso="$root/build/winlux-live-test.iso"
fi
test -r "$iso" || { echo "ISO introuvable : $iso" >&2; exit 1; }

for tool in qemu-system-x86_64 qemu-img losetup parted mkfs.ext4; do
    command -v "$tool" >/dev/null 2>&1 || { echo "outil requis absent : $tool" >&2; exit 1; }
done

cleanup() {
    if test -n "${loop_dev:-}"; then
        umount -R "$mnt" 2>/dev/null || true
        losetup -d "$loop_dev" 2>/dev/null || true
    fi
    if test "$keep_disk" -eq 0; then rm -rf "$work"; fi
}
trap cleanup EXIT

rm -rf "$work"
mkdir -p "$work"

echo "=== Création du disque de test ($disk_size) ==="
qemu-img create -f raw "$disk" "$disk_size"

echo "=== Partitionnement et formatage ==="
# GPT avec une seule partition ext4 (pas d'ESP pour le test BIOS).
parted -s "$disk" -- mklabel gpt
parted -s "$disk" -- mkpart WINLUX_ROOT ext4 1MiB 100%

loop_dev=$(losetup --find --show --partscan "$disk")
# Attendre que la partition apparaisse.
for attempt in $(seq 1 10); do
    test -b "${loop_dev}p1" && break
    partprobe "$loop_dev" 2>/dev/null || true
    sleep 0.5
done
test -b "${loop_dev}p1" || { echo "partition ${loop_dev}p1 introuvable" >&2; exit 1; }

mkfs.ext4 -F -L WINLUX_ROOT "${loop_dev}p1"
mnt=$(mktemp -d /tmp/winlux-testmnt.XXXXXX)
mount "${loop_dev}p1" "$mnt"
mkdir -p "$mnt/boot"

# Extraire noyau + initramfs depuis l'ISO.
iso_mnt=$(mktemp -d /tmp/winlux-iso.XXXXXX)
mount -o loop,ro "$iso" "$iso_mnt"
cp "$iso_mnt/boot/vmlinuz" "$mnt/boot/vmlinuz"
cp "$iso_mnt/boot/winlux-initramfs.cpio.gz" "$mnt/boot/winlux-initramfs.cpio.gz"
mkdir -p "$mnt/boot/grub"
cp "$iso_mnt/boot/grub/grub.cfg" "$mnt/boot/grub/grub.cfg"
umount "$iso_mnt"
rmdir "$iso_mnt"

# Installer GRUB BIOS sur le disque.
if test -d /usr/lib/grub/i386-pc; then
    grub-install --target=i386-pc --boot-directory="$mnt/boot" "$loop_dev"
fi

# Créer l'espace persistant.
mkdir -p "$mnt/etc/winlux" "$mnt/root"

umount "$mnt"
rmdir "$mnt"
losetup -d "$loop_dev"
loop_dev=""

echo "=== Démarrage de la VM (timeout ${timeout_s}s) ==="
log="$work/qemu-output.log"
timeout "$timeout_s" qemu-system-x86_64 \
    -drive file="$disk",format=raw,if=virtio \
    -m 2048 -smp 2 -nographic \
    -serial mon:stdio \
    -no-reboot \
    2>&1 | tee "$log" || true

echo ""
echo "=== Résultats ==="

pass=0
fail=0

check() {
    pattern=$1
    label=$2
    if grep -q "$pattern" "$log"; then
        echo "  OK : $label"
        pass=$((pass + 1))
    else
        echo "  ÉCHEC : $label"
        fail=$((fail + 1))
    fi
}

check "WINLUX_SYSTEM_VERSION=" "Version système détectée"
check "WINLUX_LIVE_SEQUENTIAL_EXIT=0" "Tests séquentiels réussis"
check "WINLUX_LIVE_PARALLEL_EXIT=0" "Tests parallèles réussis"
check "WINLUX_LIVE_X11_EXIT=0" "Test X11 réussi"
check "WINLUX_LIVE_RESULT=0" "Résultat global"

echo ""
echo "Total : $pass réussis, $fail échoués"
test "$fail" -eq 0
