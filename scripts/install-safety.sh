#!/bin/sh
# Fonctions partagées de prévalidation ; aucune écriture de disque ici.
install_die() { echo "ERREUR : $*" >&2; exit 1; }
install_confirmation_matches() { test "$1" = "EFFACER $2"; }
install_check_filesystems() {
    for fs in $1; do
        case "$fs" in btrfs|zfs_member|*raid_member|LVM2_member|crypto_LUKS|bcache)
            install_die "signature de volume composite refusée : $fs" ;;
        esac
    done
}
install_check_snapshot() {
    # type, lecture seule, octets, types descendants, major:minor, montages/swaps
    test -n "$4" && test -n "$5" && test -n "$6" || install_die 'inventaire incomplet'
    case "$1" in disk|loop) ;; *) install_die 'cible refusée : disque entier requis' ;; esac
    test "$2" = 0 || install_die 'cible en lecture seule'
    case "$3" in ''|*[!0-9]*) install_die 'taille illisible' ;; esac
    test "$3" -ge 2147483648 || install_die 'disque trop petit (2 Gio minimum)'
    for kind in $4; do
        case "$kind" in disk|loop|part) ;; *) install_die "périphérique composite actif : $kind" ;; esac
    done
    for id in $5; do
        for used in $6; do
            test "$id" != "$used" || install_die 'disque utilisé par un montage ou le swap'
        done
    done
}
install_check_device() {
    device=$1
    test -b "$device" || install_die 'la cible doit être un périphérique bloc'
    kind=$(lsblk -dnro TYPE "$device") || install_die 'type illisible'
    ro=$(lsblk -dnro RO "$device") || install_die 'état lecture seule illisible'
    size=$(lsblk -bdnro SIZE "$device") || install_die 'taille illisible'
    kinds=$(lsblk -nro TYPE "$device") || install_die 'descendants illisibles'
    ids=$(lsblk -nro MAJ:MIN "$device") || install_die 'identifiants illisibles'
    mounted=$(lsblk -nro MOUNTPOINTS "$device") || install_die 'points de montage illisibles'
    test -z "$(printf '%s' "$mounted" | tr -d '[:space:]')" || install_die 'disque ou descendant monté'
    filesystems=$(lsblk -nro FSTYPE "$device") || install_die 'signatures illisibles'
    install_check_filesystems "$filesystems"
    mounts=$(findmnt -rn -o MAJ:MIN) || install_die 'montages illisibles'
    sources=$(findmnt -rn -o SOURCE) || install_die 'sources des montages illisibles'
    for source in $sources; do
        source=${source%%\[*}
        if test -b "$source"; then
            used=$(lsblk -dnro MAJ:MIN "$source") || install_die 'source bloc illisible'
            mounts="$mounts $used"
        fi
    done
    swaps=$(swapon --noheadings --raw --show=NAME) || install_die 'swap illisible'
    for swap in $swaps; do
        if test -b "$swap"; then
            used=$(lsblk -dnro MAJ:MIN "$swap") || install_die 'swap bloc illisible'
        else
            used=$(findmnt -nro MAJ:MIN --target "$swap") || install_die 'fichier swap illisible'
        fi
        mounts="$mounts $used"
    done
    install_check_snapshot "$kind" "$ro" "$size" "$kinds" "$ids" "$mounts"
    for id in $ids; do
        test -d "/sys/dev/block/$id/holders" || install_die 'état noyau illisible'
        holders=$(find "/sys/dev/block/$id/holders" -mindepth 1 -maxdepth 1 -print)
        test -z "$holders" || install_die 'périphérique utilisé par un autre volume'
    done
}
