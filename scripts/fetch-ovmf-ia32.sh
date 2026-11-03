#!/bin/sh
# Micrologiciel UEFI 32 bits pour QEMU (tests seulement, jamais dans l'image).
#
#   sh scripts/fetch-ovmf-ia32.sh [DOSSIER]
#
# Ubuntu 26.04 ne fournit plus le paquet ovmf-ia32 : on prend celui d'Ubuntu
# 24.04 (version de sortie, toujours présente dans l'archive), vérifié par son
# SHA-256, puis extrait sans l'installer. Affiche le dossier qui contient
# OVMF32_CODE_4M.fd et OVMF32_VARS_4M.fd. Utilise le paquet système s'il existe.
set -eu
if test -f /usr/share/OVMF/OVMF32_CODE_4M.fd && test -f /usr/share/OVMF/OVMF32_VARS_4M.fd; then
    echo /usr/share/OVMF
    exit 0
fi
dir=${1:-build/ovmf-ia32}
url=http://archive.ubuntu.com/ubuntu/pool/universe/e/edk2/ovmf-ia32_2024.02-2_all.deb
sum=2333c7555133a1089a4d63007e2e0484a63ebf5d4cde86bc7519add3a1d18b8d
if ! test -f "$dir/usr/share/OVMF/OVMF32_CODE_4M.fd"; then
    mkdir -p "$dir"
    curl -fsSL --retry 5 -o "$dir/ovmf-ia32.deb" "$url"
    echo "$sum  $dir/ovmf-ia32.deb" | sha256sum -c --quiet - || { echo "ovmf-ia32 : somme SHA-256 inattendue" >&2; exit 1; }
    dpkg-deb -x "$dir/ovmf-ia32.deb" "$dir"
    rm -f "$dir/ovmf-ia32.deb"
fi
for name in OVMF32_CODE_4M.fd OVMF32_VARS_4M.fd; do
    test -f "$dir/usr/share/OVMF/$name" || { echo "ovmf-ia32 : $name absent" >&2; exit 1; }
done
(cd "$dir/usr/share/OVMF" && pwd)
