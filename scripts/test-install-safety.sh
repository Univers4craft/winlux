#!/bin/sh
# Tests purs : ni root, ni accès en écriture à un périphérique.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$root/scripts/install-safety.sh"
reject() {
    if (install_check_snapshot "$@") >/dev/null 2>&1; then
        echo 'ERREUR : cible dangereuse acceptée' >&2; exit 1
    fi
}
install_check_snapshot disk 0 4294967296 'disk part' '8:0 8:1' '259:1'
install_check_snapshot loop 0 4294967296 'loop part' '7:0 259:5' '259:1'
reject part 0 4294967296 part '8:1' '259:1'
reject disk 1 4294967296 disk '8:0' '259:1'
reject disk 0 1024 disk '8:0' '259:1'
reject disk 0 invalid disk '8:0' '259:1'
reject disk 0 4294967296 'disk part crypt' '8:0 8:1 253:0' '259:1'
reject disk 0 4294967296 'disk part' '8:0 8:1' '259:1 8:1'
reject disk 0 4294967296 disk '8:0' ''
reject disk 0 4294967296 disk '' '259:1'
install_confirmation_matches 'EFFACER /dev/loop99' /dev/loop99
if install_confirmation_matches 'EFFACER /dev/sda' /dev/sdb; then exit 1; fi
if install_confirmation_matches yes /dev/sda; then exit 1; fi
install_check_filesystems 'ext4 vfat'
for fs in btrfs crypto_LUKS LVM2_member linux_raid_member zfs_member bcache; do
    if (install_check_filesystems "$fs") >/dev/null 2>&1; then exit 1; fi
done
echo 'OK : refus partitions, RO, taille, volumes composites, montages/swap, inventaire incomplet et confirmation exacte'
