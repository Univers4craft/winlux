#!/bin/sh
# Identité de l'image installée : Winlux, « construit à partir de paquets Ubuntu ».
#
#   sh scripts/winlux-brand-image.sh RACINE [LOGO.svg]
#
# La politique de marque de Canonical interdit de présenter un système modifié
# comme « Ubuntu » (docs/RISQUES-JURIDIQUES.md, section 6). On remplace donc :
#   - /usr/lib/os-release (lu par /etc/os-release), /etc/lsb-release,
#     /etc/issue, /etc/issue.net et /etc/legal : nom Winlux. ID_LIKE et
#     UBUNTU_CODENAME restent, car les outils qui cherchent les paquets Ubuntu
#     compatibles (dépôts, Flatpak, pilotes) en ont besoin ;
#   - les logos Ubuntu (distributor-logo, ubuntu-logo, start-here de Yaru,
#     thèmes Plymouth Ubuntu) : retirés, et le logo Prisme sert de
#     « distributor-logo ».
# Chaque fichier de base-files est détourné avec dpkg-divert, et les logos
# sont exclus par dpkg : une mise à jour APT ne remet ni le nom ni les logos.
set -eu
root=${1:?racine de l\'image requise}
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
logo=${2:-$project/assets/brand/winlux-logo.svg}
test -d "$root/etc" || { echo "racine invalide : $root" >&2; exit 1; }

divert() {
    # Fichier d'Ubuntu mis de côté en « .ubuntu », celui de Winlux à sa place.
    target=$1
    if command -v dpkg-divert >/dev/null 2>&1 && test -d "$root/var/lib/dpkg"; then
        dpkg-divert --root "$root" --local --rename --divert "$target.ubuntu" --add "$target" >/dev/null
    elif test -e "$root$target" && ! test -e "$root$target.ubuntu"; then
        mv "$root$target" "$root$target.ubuntu"
    fi
    rm -f "$root$target"
    cat >"$root$target"
    chmod 644 "$root$target"
}

codename=
for file in "$root/usr/lib/os-release.ubuntu" "$root/usr/lib/os-release" "$root/etc/os-release"; do
    test -r "$file" || continue
    codename=$(sed -n 's/^UBUNTU_CODENAME=//p; s/^VERSION_CODENAME=//p' "$file" | tr -d '"' | head -n 1)
    test -n "$codename" && break
done
mkdir -p "$root/usr/lib"
divert /usr/lib/os-release <<EOF
NAME="Winlux"
PRETTY_NAME="Winlux"
ID=winlux
ID_LIKE="ubuntu debian"
VERSION_CODENAME=$codename
UBUNTU_CODENAME=$codename
HOME_URL="https://univers4craft.github.io/winlux/"
SUPPORT_URL="https://github.com/Univers4craft/winlux/wiki"
BUG_REPORT_URL="https://github.com/Univers4craft/winlux/issues"
PRIVACY_POLICY_URL="https://univers4craft.github.io/winlux/fr/privacy.html"
LOGO=distributor-logo
EOF
# Lien d'origine de base-files (/etc/os-release -> ../usr/lib/os-release).
if ! test -L "$root/etc/os-release"; then
    rm -f "$root/etc/os-release"
    ln -s ../usr/lib/os-release "$root/etc/os-release"
fi
divert /etc/lsb-release <<'EOF'
DISTRIB_ID=Winlux
DISTRIB_DESCRIPTION="Winlux"
EOF
printf 'Winlux \\n \\l\n\n' | divert /etc/issue
printf 'Winlux\n' | divert /etc/issue.net
divert /etc/legal <<'EOF'

Winlux est construit à partir de paquets Ubuntu. Ces logiciels sont libres ;
les conditions de distribution de chacun se trouvent dans
/usr/share/doc/*/copyright, et la liste des sources correspondantes des
logiciels sous GPL dans /usr/share/doc/winlux/sources-gpl.tsv.

Winlux n'est ni affilié à Canonical ni approuvé par Canonical. Ubuntu est
une marque déposée de Canonical Ltd. Winlux n'est pas non plus affilié à
Microsoft.

EOF

# Logos Ubuntu : retirés de l'image et exclus des futures installations.
patterns='/usr/share/icons/*/*/*/distributor-logo*
/usr/share/icons/*/*/*/ubuntu-logo*
/usr/share/icons/*/*/*/*ubuntu-logo*
/usr/share/icons/Yaru/*/places/start-here*
/usr/share/pixmaps/ubuntu-logo*
/usr/share/pixmaps/*ubuntu*logo*
/usr/share/plymouth/ubuntu-logo*
/usr/share/plymouth/themes/ubuntu-*'
mkdir -p "$root/etc/dpkg/dpkg.cfg.d"
{
    echo '# Winlux : pas de logo Ubuntu dans l'"'"'image (scripts/winlux-brand-image.sh).'
    printf '%s\n' "$patterns" | sed 's/^/path-exclude=/'
} >"$root/etc/dpkg/dpkg.cfg.d/winlux-brand"
printf '%s\n' "$patterns" | while IFS= read -r pattern; do
    for path in "$root"$pattern; do
        test -e "$path" || test -L "$path" || continue
        rm -rf "$path"
        echo "logo Ubuntu retiré : ${path#"$root"}"
    done
done
# Logo Prisme sous le nom standard « distributor-logo » (os-release LOGO=).
if test -r "$logo"; then
    install -D -m 644 "$logo" "$root/usr/share/icons/hicolor/scalable/apps/distributor-logo.svg"
    install -D -m 644 "$logo" "$root/usr/share/pixmaps/distributor-logo.svg"
fi
for size in 16 32 48 64 128 256; do
    png="$project/assets/brand/winlux-logo-$size.png"
    test -r "$png" && install -D -m 644 "$png" "$root/usr/share/icons/hicolor/${size}x$size/apps/distributor-logo.png"
done
echo "Identité de l'image : Winlux (construit à partir de paquets Ubuntu)"
