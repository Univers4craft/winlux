#!/bin/sh
# Chaîne Secure Boot de Winlux : shim signé -> GRUB signé -> noyau signé.
# Voir docs/SECURE-BOOT.md.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH

: "${WINLUX_SHIM:=/usr/lib/shim/shimx64.efi.signed}"
: "${WINLUX_MOKMANAGER:=/usr/lib/shim/mmx64.efi}"
: "${WINLUX_SIGNED_GRUB:=/usr/lib/grub/x86_64-efi-signed/gcdx64.efi.signed}"
: "${WINLUX_SIGNED_GRUB_DISK:=/usr/lib/grub/x86_64-efi-signed/grubx64.efi.signed}"
: "${WINLUX_MOK_DIR:=/var/lib/winlux/secureboot}"

die() { echo "winlux-secureboot : $*" >&2; exit 1; }

usage() {
    cat <<'EOF'
Usage : winlux-secureboot COMMANDE

  status                      état Secure Boot du micrologiciel et clés MOK
  check                       vérifie que shim, MokManager et GRUB signés sont présents
  prepare-iso ISO_ROOT LABEL  ajoute shim, GRUB signé et MokManager à l'arborescence de l'ISO
  make-iso ISO_ROOT OUT       crée l'ISO hybride BIOS + UEFI Secure Boot
  install-esp ESP_DIR UUID    installe shim + GRUB signé sur l'ESP d'un disque
  gen-mok [DIR]               crée une clé MOK (RSA 2048, 10 ans) pour signer noyau et modules
  sign-kernel VMLINUZ [DIR]   signe un noyau avec la clé MOK (sbsign)
  enroll [DIR]                demande l'enregistrement de la clé MOK au prochain démarrage
EOF
}

first_existing() {
    for candidate in "$@"; do
        if test -f "$candidate"; then printf '%s\n' "$candidate"; return 0; fi
    done
    return 1
}

check_files() {
    test -f "$WINLUX_SHIM" || die "shim signé absent : $WINLUX_SHIM (paquet shim-signed)"
    test -f "$WINLUX_MOKMANAGER" || die "MokManager absent : $WINLUX_MOKMANAGER (paquet shim-signed)"
    test -f "$1" || die "GRUB signé absent : $1 (paquet grub-efi-amd64-signed)"
}

# The signed GRUB only reads grub.cfg next to itself (or its built-in prefix),
# so this stub points it at the real configuration.
write_stub() {
    destination=$1
    search=$2
    cat >"$destination" <<EOF
search --no-floppy --set=root $search
set prefix=(\$root)/boot/grub
configfile \$prefix/grub.cfg
EOF
}

copy_chain() {
    efi_boot=$1
    grub=$2
    mkdir -p "$efi_boot"
    cp "$WINLUX_SHIM" "$efi_boot/BOOTX64.EFI"
    cp "$grub" "$efi_boot/grubx64.efi"
    cp "$WINLUX_MOKMANAGER" "$efi_boot/mmx64.efi"
}

secureboot_state() {
    variable=/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c
    test -d /sys/firmware/efi || { echo 'Démarrage : BIOS (Secure Boot non applicable)'; return; }
    if test -r "$variable" && test "$(od -An -t u1 -j 4 -N 1 "$variable" | tr -d ' ')" = 1; then
        echo 'Démarrage : UEFI, Secure Boot ACTIVÉ'
    else
        echo 'Démarrage : UEFI, Secure Boot désactivé'
    fi
}

command=${1:-}
test $# -gt 0 && shift
case "$command" in
    status)
        secureboot_state
        if command -v mokutil >/dev/null 2>&1; then
            mokutil --sb-state 2>/dev/null || true
            echo 'Clés MOK enregistrées :'
            mokutil --list-enrolled 2>/dev/null | grep -E 'Subject:|CN=' || echo '  (aucune)'
        fi ;;
    check)
        check_files "$WINLUX_SIGNED_GRUB"
        echo "shim : $WINLUX_SHIM"
        echo "MokManager : $WINLUX_MOKMANAGER"
        echo "GRUB signé : $WINLUX_SIGNED_GRUB" ;;
    prepare-iso)
        iso_root=${1:?arborescence ISO requise}
        label=${2:-Winlux}
        test -f "$iso_root/boot/grub/grub.cfg" || die "grub.cfg absent de $iso_root/boot/grub"
        check_files "$WINLUX_SIGNED_GRUB"
        test -d /usr/lib/grub/i386-pc || die 'modules GRUB BIOS absents (paquet grub-pc-bin)'
        copy_chain "$iso_root/EFI/BOOT" "$WINLUX_SIGNED_GRUB"
        write_stub "$iso_root/EFI/BOOT/grub.cfg" '--file /boot/winlux-rootfs.squashfs'
        # Ubuntu's signed GRUB has /EFI/ubuntu as its built-in prefix.
        mkdir -p "$iso_root/EFI/ubuntu"
        cp "$iso_root/EFI/BOOT/grub.cfg" "$iso_root/EFI/ubuntu/grub.cfg"
        # The signed CD GRUB locates the medium through /.disk/info, then reads x86_64-efi/grub.cfg.
        mkdir -p "$iso_root/.disk" "$iso_root/boot/grub/x86_64-efi"
        printf '%s\n' "$label" >"$iso_root/.disk/info"
        printf 'configfile /boot/grub/grub.cfg\n' >"$iso_root/boot/grub/x86_64-efi/grub.cfg"
        # xorriso is used directly, so the BIOS modules grub-mkrescue would add are copied here.
        rm -rf "$iso_root/boot/grub/i386-pc"
        cp -a /usr/lib/grub/i386-pc "$iso_root/boot/grub/i386-pc" ;;
    make-iso)
        iso_root=${1:?arborescence ISO requise}
        out=${2:?ISO de sortie requise}
        for tool in mkfs.fat mcopy mmd grub-mkimage xorriso; do
            command -v "$tool" >/dev/null 2>&1 || die "outil requis absent : $tool (paquets mtools, dosfstools, grub-common, xorriso)"
        done
        test -f "$iso_root/EFI/BOOT/BOOTX64.EFI" || die 'lancez d abord prepare-iso'
        efi_img="$iso_root/boot/grub/efi.img"
        rm -f "$efi_img"
        esp_kib=$(du -sk "$iso_root/EFI" | awk '{ print $1 + 2048 }')
        truncate -s "${esp_kib}K" "$efi_img"
        mkfs.fat -n WINLUX_EFI "$efi_img" >/dev/null
        mmd -i "$efi_img" ::/EFI ::/EFI/BOOT ::/EFI/ubuntu
        mcopy -i "$efi_img" "$iso_root/EFI/BOOT/"* ::/EFI/BOOT/
        mcopy -i "$efi_img" "$iso_root/EFI/ubuntu/grub.cfg" ::/EFI/ubuntu/
        grub-mkimage -O i386-pc-eltorito -p /boot/grub -o "$iso_root/boot/grub/bios.img" \
            biosdisk iso9660 part_gpt part_msdos normal search search_label search_fs_file search_fs_uuid \
            configfile linux echo test sleep serial terminal gzio
        xorriso -as mkisofs -r -J -joliet-long -V WINLUX -o "$out" \
            --grub2-mbr /usr/lib/grub/i386-pc/boot_hybrid.img -partition_offset 16 \
            -b boot/grub/bios.img -no-emul-boot -boot-load-size 4 -boot-info-table --grub2-boot-info \
            -eltorito-alt-boot -e boot/grub/efi.img -no-emul-boot \
            -append_partition 2 0xef "$efi_img" -appended_part_as_gpt \
            "$iso_root" ;;
    install-esp)
        esp_dir=${1:?point de montage ESP requis}
        uuid=${2:?UUID de la racine requis}
        case "$uuid" in ''|*[!a-fA-F0-9-]*) die "UUID invalide : $uuid" ;; esac
        grub=$(first_existing "$WINLUX_SIGNED_GRUB_DISK" "$WINLUX_SIGNED_GRUB") || die 'GRUB signé absent (paquet grub-efi-amd64-signed)'
        check_files "$grub"
        copy_chain "$esp_dir/EFI/BOOT" "$grub"
        write_stub "$esp_dir/EFI/BOOT/grub.cfg" "--fs-uuid $uuid"
        mkdir -p "$esp_dir/EFI/ubuntu"
        cp "$esp_dir/EFI/BOOT/grub.cfg" "$esp_dir/EFI/ubuntu/grub.cfg"
        echo "Chaîne Secure Boot installée dans $esp_dir/EFI/BOOT" ;;
    gen-mok)
        dir=${1:-$WINLUX_MOK_DIR}
        command -v openssl >/dev/null 2>&1 || die 'openssl requis'
        test ! -e "$dir/MOK.key" || die "une clé existe déjà : $dir/MOK.key"
        mkdir -p "$dir"
        chmod 700 "$dir"
        umask 077
        openssl req -new -x509 -newkey rsa:2048 -sha256 -nodes -days 3650 \
            -subj "/CN=Winlux local Secure Boot key ($(hostname 2>/dev/null || echo winlux))/" \
            -addext 'extendedKeyUsage=codeSigning' \
            -keyout "$dir/MOK.key" -out "$dir/MOK.crt"
        openssl x509 -in "$dir/MOK.crt" -outform DER -out "$dir/MOK.der"
        echo "Clé MOK créée dans $dir (MOK.key privée, MOK.der à enregistrer)." ;;
    sign-kernel)
        kernel=${1:?noyau requis}
        dir=${2:-$WINLUX_MOK_DIR}
        command -v sbsign >/dev/null 2>&1 || die 'sbsign requis (paquet sbsigntool)'
        test -r "$dir/MOK.key" && test -r "$dir/MOK.crt" || die "clé MOK absente : lancez « winlux-secureboot gen-mok »"
        sbsign --key "$dir/MOK.key" --cert "$dir/MOK.crt" --output "$kernel.signed" "$kernel"
        if command -v sbverify >/dev/null 2>&1; then sbverify --cert "$dir/MOK.crt" "$kernel.signed"; fi
        mv "$kernel.signed" "$kernel"
        echo "Noyau signé : $kernel" ;;
    enroll)
        dir=${1:-$WINLUX_MOK_DIR}
        command -v mokutil >/dev/null 2>&1 || die 'mokutil requis'
        test -r "$dir/MOK.der" || die "certificat absent : $dir/MOK.der"
        echo 'Choisissez un mot de passe temporaire : MokManager le redemandera au prochain démarrage.'
        mokutil --import "$dir/MOK.der"
        echo 'Redémarrez, puis dans l écran bleu MokManager : Enroll MOK > Continue > Yes > mot de passe.' ;;
    -h|--help|help|'') usage ;;
    *) usage >&2; exit 1 ;;
esac
