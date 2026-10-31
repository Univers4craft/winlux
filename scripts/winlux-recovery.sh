#!/bin/sh
# Winlux Recovery : menu de secours, exécuté par l'initramfs (winlux.recovery=1)
# dans un chroot sur la racine installée (/dev, /proc, /sys déjà montés).
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH

msg()  { printf '\033[1;32m[*]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;31m[!]\033[0m %s\n' "$*"; }

cmdline_value() {
    for argument in $(cat /proc/cmdline); do
        case "$argument" in "$1"=*) printf '%s\n' "${argument#*=}"; return 0 ;; esac
    done
    return 1
}

root_partition() {
    device=$(cmdline_value root) || return 1
    case "$device" in UUID=*|LABEL=*|PARTUUID=*) device=$(findfs "$device" 2>/dev/null) || return 1 ;; esac
    test -b "$device" && printf '%s\n' "$device"
}

parent_disk() {
    name=$(basename "$1")
    parent=$(basename "$(readlink -f "/sys/class/block/$name/..")")
    test -b "/dev/$parent" && test "$parent" != "$name" && printf '/dev/%s\n' "$parent"
}

esp_partition() {
    for part in "${1}2" "${1}p2"; do
        if test -b "$part" && test "$(blkid -s TYPE -o value "$part" 2>/dev/null)" = vfat; then
            printf '%s\n' "$part"
            return 0
        fi
    done
    return 1
}

repair_bootloader() {
    part=$(root_partition) || { warn 'Partition racine introuvable.'; return 1; }
    disk=$(parent_disk "$part") || { warn "Disque parent de $part introuvable."; return 1; }
    command -v grub-install >/dev/null 2>&1 || {
        warn "grub-install absent de ce système : démarrez la clé USB Winlux et choisissez « Winlux Recovery »."
        return 1
    }
    msg "Disque : $disk — racine : $part"
    grub-install --target=i386-pc --boot-directory=/boot --recheck "$disk" || warn 'GRUB BIOS non réinstallé (normal en UEFI seul).'
    if esp=$(esp_partition "$disk"); then
        mkdir -p /boot/efi
        mount -t vfat -o iocharset=ascii "$esp" /boot/efi || { warn "Montage ESP $esp impossible."; return 1; }
        grub-install --target=x86_64-efi --efi-directory=/boot/efi --boot-directory=/boot --removable --no-nvram --recheck || warn 'GRUB UEFI non réinstallé.'
        if test -x /usr/bin/winlux-secureboot; then
            /usr/bin/winlux-secureboot install-esp /boot/efi "$(blkid -s UUID -o value "$part")" || warn 'Chaîne Secure Boot non réinstallée.'
        fi
        umount /boot/efi
    else
        warn 'Aucune partition ESP détectée : démarrage UEFI non réparé.'
    fi
    grub-script-check /boot/grub/grub.cfg 2>/dev/null || warn '/boot/grub/grub.cfg invalide ou absent.'
    sync
    msg 'Réparation terminée.'
}

restore_snapshot() {
    test -x /usr/bin/winlux-update || { warn 'winlux-update absent.'; return 1; }
    msg 'Restauration du point de restauration précédent...'
    if /usr/bin/winlux-update --rollback; then msg 'Restauration terminée. Redémarrez.'; else warn 'Aucun point de restauration valide.'; return 1; fi
}

rescue_terminal() {
    msg "Terminal de secours sur le système installé. Tapez « exit » pour revenir au menu."
    PS1='(winlux-rescue) # ' /bin/sh -i
}

check_filesystem() {
    part=$(root_partition) || { warn 'Partition racine introuvable.'; return 1; }
    command -v e2fsck >/dev/null 2>&1 || command -v fsck.ext4 >/dev/null 2>&1 || {
        warn 'e2fsck absent : vérifiez depuis la clé USB Winlux.'
        return 1
    }
    fsck_tool=$(command -v e2fsck || command -v fsck.ext4)
    sync
    mount -o remount,ro / || { warn 'Impossible de passer la racine en lecture seule ; fsck annulé.'; return 1; }
    msg "Vérification de $part..."
    "$fsck_tool" -f -y "$part"
    status=$?
    mount -o remount,rw / || warn 'Remontage en écriture impossible : redémarrez.'
    case $status in
        0) msg 'Aucune erreur.' ;;
        1|2) msg 'Erreurs corrigées. Redémarrez pour appliquer.' ;;
        *) warn "e2fsck a terminé avec le code $status : erreurs non corrigées." ;;
    esac
}

echo
echo '=========================================='
echo '        Winlux — Mode récupération'
echo '=========================================='
while :; do
    echo
    echo '  1) Réparer le démarrage (GRUB)'
    echo '  2) Restaurer le point de restauration précédent'
    echo '  3) Terminal de secours'
    echo '  4) Vérifier le système de fichiers (fsck)'
    echo '  5) Redémarrer'
    echo '  6) Éteindre'
    printf '> '
    read -r choice || choice=5
    case "$choice" in
        1) repair_bootloader ;;
        2) restore_snapshot ;;
        3) rescue_terminal ;;
        4) check_filesystem ;;
        5) sync; reboot -f ;;
        6) sync; poweroff -f ;;
        *) warn 'Choix invalide.' ;;
    esac
done
