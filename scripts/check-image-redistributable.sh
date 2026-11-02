#!/bin/sh
# Refuse une image qui contient un logiciel d'éditeur tiers non redistribuable.
#
#   sh scripts/check-image-redistributable.sh RACINE
#
# L'ISO publiée ne contient que des logiciels libres ou redistribuables
# (archive Ubuntu, Firefox officiel, Winlux). Les logiciels propriétaires
# dont la licence interdit la redistribution, comme Microsoft Edge
# (« you will not ... share, publish, distribute, or lend the Software »,
# section 2.8 f) ou Google Chrome, se téléchargent chez leur éditeur après
# l'installation, à la demande de l'utilisateur. Une seule trace (dossier
# d'installation, paquet dpkg, .deb mis en cache) fait échouer la construction.
set -eu
root=${1:?racine de l\'image requise}
test -d "$root" || { echo "racine absente : $root" >&2; exit 1; }
found=

# Dossiers d'installation des éditeurs.
for path in opt/microsoft opt/google opt/brave.com opt/vivaldi opt/opera opt/zoom opt/teams \
    opt/discord opt/spotify usr/share/microsoft-edge usr/share/teams usr/share/code \
    usr/bin/microsoft-edge usr/bin/microsoft-edge-stable usr/bin/google-chrome usr/bin/google-chrome-stable \
    usr/share/winlux/browsers/edge.deb usr/share/winlux/browsers/chrome.deb; do
    if test -e "$root/$path" || test -L "$root/$path"; then found="$found /$path"; fi
done

# Paquets propriétaires connus, installés (dpkg) ou mis en cache (.deb).
denied='microsoft-*|msedge*|google-chrome*|google-earth*|code|code-insiders|teams|teams-for-linux|skypeforlinux|vivaldi*|opera-stable|opera-beta|brave-browser*|zoom|discord|spotify-client|steam-launcher|yandex-browser*|anydesk|teamviewer*'
status="$root/var/lib/dpkg/status"
if test -r "$status"; then
    packages=$(awk '/^Package: / { name = $2 } /^Status: .* installed$/ { print name }' "$status")
    for name in $packages; do
        # shellcheck disable=SC2254 # motifs volontairement non cités
        eval "case \"\$name\" in $denied) found=\"\$found paquet:\$name\" ;; esac"
    done
fi
list=$(mktemp)
trap 'rm -f "$list"' EXIT HUP INT TERM
find "$root" -xdev -type f -name '*.deb' 2>/dev/null >"$list" || true
while IFS= read -r deb; do
    name=$(dpkg-deb -f "$deb" Package 2>/dev/null || basename "$deb" .deb)
    eval "case \"\$name\" in $denied) found=\"\$found deb:\${deb#\"\$root\"}\" ;; esac"
done <"$list"

if test -n "$found"; then
    echo "Image refusée : logiciel propriétaire non redistribuable :$found" >&2
    echo "Il doit être téléchargé chez son éditeur après l'installation, à la demande." >&2
    exit 1
fi
echo "Image redistribuable : aucun logiciel propriétaire d'éditeur tiers"
