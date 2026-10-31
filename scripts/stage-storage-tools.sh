#!/bin/sh
# Ajoute à la racine live les outils et modules facultatifs des archives et
# des images disque (Explorer : archives 7z/RAR/xz/zstd, montage iso/vhd/qcow2/dmg).
# Tout est facultatif : un outil absent de la machine de construction n'est
# simplement pas copié, et Winlux ne proposera pas le format correspondant.
# Usage : stage-storage-tools.sh <racine> <version-noyau>
set -eu
stage=$1
kernel_release=$2

copy_libs() {
    ldd "$1" 2>/dev/null | awk '$2 == "=>" && $3 ~ /^\// { print $3 } $1 ~ /^\// { print $1 }' | sort -u |
    while IFS= read -r lib; do
        test -n "$lib" || continue
        mkdir -p "$stage$(dirname -- "$lib")"
        test -e "$stage$lib" || cp -L "$lib" "$stage$lib"
    done
}

mkdir -p "$stage/usr/bin"
for tool in 7zz 7z 7za unrar unar lsar bsdtar xz zstd bzip2 qemu-nbd dmg2img losetup blkid udisksctl; do
    path=$(command -v "$tool" 2>/dev/null || true)
    test -n "$path" || continue
    # 7z/7za de p7zip sont des scripts qui appellent /usr/lib/p7zip.
    case "$tool" in
        7z | 7za)
            if test -d /usr/lib/p7zip; then
                mkdir -p "$stage/usr/lib"
                cp -a /usr/lib/p7zip "$stage/usr/lib/"
                for helper in /usr/lib/p7zip/*; do test -f "$helper" && copy_libs "$helper"; done
            fi
            ;;
    esac
    rm -f "$stage/usr/bin/$tool"
    cp -L "$path" "$stage/usr/bin/$tool"
    chmod 755 "$stage/usr/bin/$tool"
    copy_libs "$path"
done

# Modules noyau : boucle, nbd et systèmes de fichiers des images courantes.
for driver in loop nbd isofs udf hfsplus ntfs3 exfat squashfs; do
    modprobe -S "$kernel_release" --show-depends "$driver" 2>/dev/null | while read -r directive module rest; do
        test "$directive" = insmod || continue
        destination="$stage${module%.zst}"
        destination="${destination%.xz}"
        mkdir -p "$(dirname -- "$destination")"
        case "$module" in
            *.zst) zstd -q -d -c "$module" >"$destination" ;;
            *.xz) xz -d -c "$module" >"$destination" ;;
            *) cp "$module" "$destination" ;;
        esac
    done
done
