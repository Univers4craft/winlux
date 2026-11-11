#!/bin/sh
# Navigateurs ajoutés au choix le 11 novembre 2026 (#1716) : Brave, Opera,
# Opera GX, Tor Browser et Vivaldi, tous avec une version Linux officielle.
#  - Brave : paquet .deb rangé comme celui de l'éditeur (chemin habituel).
#  - Opera GX : programme dans un dossier inhabituel, trouvé par le lien que
#    le paquet pose dans /usr/bin ; nom de paquet contrôlé.
#  - Tor Browser : archive .tar.xz acceptée seulement avec la signature de la
#    clé épinglée (une autre clé, une signature absente ou un chemin « .. »
#    sont refusés), lancée depuis une copie personnelle qui garde le profil.
set -eu
umask 022
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
export WINLUX_BROWSER_ROOT="$work/system"
export WINLUX_BROWSER_TEST_LOG="$work/arguments"
export WINLUX_NETWORK_ROUTES="$work/routes"
printf 'Iface\tDestination\tGateway\neth0\t00000000\t0202000A\n' > "$work/routes"
cache="$WINLUX_BROWSER_ROOT/usr/share/winlux/browsers"
installed="$WINLUX_BROWSER_ROOT/opt/winlux/browsers"
mkdir -p "$cache"
manager() { sh "$project/scripts/winlux-browsers.sh" "$@"; }
fail() { echo "ÉCHEC : $*" >&2; exit 1; }
logger='#!/bin/sh
printf "%s\n" "$@" > "$WINLUX_BROWSER_TEST_LOG"
'
deb() {  # deb ID PAQUET CHEMIN [LIEN_USR_BIN]
    tree="$work/tree-$1"
    mkdir -p "$tree/DEBIAN" "$tree/$(dirname "$3")"
    printf 'Package: %s\nVersion: 1.0\nArchitecture: amd64\nMaintainer: test <test@example.invalid>\nDescription: browser fixture\n' "$2" > "$tree/DEBIAN/control"
    printf '%s' "$logger" > "$tree/$3"
    chmod 755 "$tree/$3"
    if test -n "${4:-}"; then mkdir -p "$tree/usr/bin"; ln -s "/$3" "$tree/usr/bin/$4"; fi
    dpkg-deb --build --root-owner-group "$tree" "$cache/$1.deb" >/dev/null
    sha256sum "$cache/$1.deb" | cut -d' ' -f1 > "$cache/$1.sha256"
    printf '1.0\n' > "$cache/$1.version"
}

# Statut : les huit navigateurs, ordre alphabétique des noms, aucun par défaut.
manager --status > "$work/status"
test "$(grep -v '^browser_default=' "$work/status" | sed -n 's/^browser_\([a-z]*\)=.*/\1/p' | tr '\n' ' ')" = 'brave chrome edge firefox opera operagx tor vivaldi ' \
    || fail "liste des navigateurs : $(cat "$work/status")"
grep -qx 'browser_default=none' "$work/status" || fail 'un navigateur est par défaut sans choix'

# Brave : chemin habituel du paquet de l'éditeur.
deb brave brave-browser opt/brave.com/brave/brave-browser
manager --default brave
grep -qx 'browser_brave=installed' "$(manager --status > "$work/s"; echo "$work/s")" || fail 'Brave non installé'
test "$(cat "$installed/brave/program")" = root/opt/brave.com/brave/brave-browser || fail 'programme de Brave'
manager --open 'https://example.invalid/brave'
grep -Fxq 'https://example.invalid/brave' "$work/arguments" || fail 'lien non ouvert dans Brave'

# Opera GX : dossier inhabituel, trouvé par /usr/bin/opera-gx.
deb operagx opera-gx-stable usr/lib/x86_64-linux-gnu/opera_gx_autre/opera_gx opera-gx
manager --install operagx
test "$(cat "$installed/operagx/program")" = root/usr/lib/x86_64-linux-gnu/opera_gx_autre/opera_gx || fail "programme d'Opera GX : $(cat "$installed/operagx/program")"
manager --launch operagx 'https://example.invalid/gx'
grep -Fxq 'https://example.invalid/gx' "$work/arguments" || fail 'lien non ouvert dans Opera GX'
# Un .deb au nom d'un autre éditeur est refusé.
rm -rf "$installed/vivaldi"
deb vivaldi brave-browser opt/vivaldi/vivaldi
if manager --install vivaldi 2>"$work/err"; then fail 'paquet au mauvais nom accepté'; fi
grep -q 'Paquet inattendu' "$work/err" || fail "message du paquet inattendu : $(cat "$work/err")"
test ! -e "$installed/vivaldi" || fail 'Vivaldi installé malgré un mauvais paquet'
deb vivaldi vivaldi-stable opt/vivaldi/vivaldi
manager --install vivaldi

# Tor Browser : archive signée.
if ! command -v gpg >/dev/null 2>&1 || ! command -v gpgv >/dev/null 2>&1; then
    echo 'gpg/gpgv absents : partie Tor Browser du test sautée' >&2
    echo 'OK : Brave, Opera GX et Vivaldi installés et ouverts (#1716)'
    exit 0
fi
export GNUPGHOME="$work/gnupg"
mkdir -m 700 "$GNUPGHOME"
gpg --batch --quiet --pinentry-mode loopback --passphrase '' --quick-gen-key 'Tor fixture <tor@example.invalid>' ed25519 sign never 2>/dev/null
gpg --batch --quiet --pinentry-mode loopback --passphrase '' --quick-gen-key 'Autre <autre@example.invalid>' ed25519 sign never 2>/dev/null
good=$(gpg --batch --with-colons --list-keys tor@example.invalid | awk -F: '/^fpr:/ {print $10; exit}')
other=$(gpg --batch --with-colons --list-keys autre@example.invalid | awk -F: '/^fpr:/ {print $10; exit}')
export WINLUX_BROWSER_TOR_FINGERPRINT="$good"
mkdir -p "$work/tor/tor-browser/Browser/TorBrowser/Data"
printf '%s' "$logger" > "$work/tor/tor-browser/Browser/start-tor-browser"
chmod 755 "$work/tor/tor-browser/Browser/start-tor-browser"
tor_archive() {  # tor_archive CLÉ [RÉPERTOIRE]
    tar -cJf "$cache/tor.tar.xz" -C "${2:-$work/tor}" .
    sha256sum "$cache/tor.tar.xz" | cut -d' ' -f1 > "$cache/tor.sha256"
    rm -f "$cache/tor.tar.xz.asc"
    gpg --batch --quiet --yes --armor --local-user "$1" --detach-sign --output "$cache/tor.tar.xz.asc" "$cache/tor.tar.xz"
    gpg --batch --export "$good" "$other" > "$cache/tor.keyring"
    printf '16.0.1\n' > "$cache/tor.version"
}
# Signée par une autre clé que celle épinglée : refusée.
tor_archive "$other"
if manager --install tor 2>"$work/err"; then fail 'Tor Browser signé par une autre clé accepté'; fi
grep -q 'clé inattendue' "$work/err" || fail "message de clé inattendue : $(cat "$work/err")"
test ! -e "$installed/tor" || fail 'Tor Browser installé malgré la signature'
# Sans signature : refusée.
tor_archive "$good"
mv "$cache/tor.tar.xz.asc" "$work/asc"
if manager --install tor 2>/dev/null; then fail 'Tor Browser sans signature accepté'; fi
mv "$work/asc" "$cache/tor.tar.xz.asc"
# Archive modifiée après signature (SHA-256 recalculé) : refusée.
cp "$cache/tor.tar.xz" "$work/tor.saved"
printf 'x' >> "$cache/tor.tar.xz"
sha256sum "$cache/tor.tar.xz" | cut -d' ' -f1 > "$cache/tor.sha256"
if manager --install tor 2>/dev/null; then fail 'Tor Browser modifié accepté'; fi
# Chemin hors du dossier tor-browser : refusé même bien signé.
mkdir -p "$work/evil/tor-browser"
printf 'x' > "$work/evil/autre-fichier"
tor_archive "$good" "$work/evil"
if manager --install tor 2>"$work/err"; then fail 'archive Tor inattendue acceptée'; fi
grep -q 'Archive de Tor Browser inattendue' "$work/err" || fail "message d'archive : $(cat "$work/err")"
# Bonne archive : installée, puis lancée depuis une copie personnelle.
tor_archive "$good"
manager --default tor
manager --status | grep -qx 'browser_tor=installed' || fail 'Tor Browser non installé'
manager --status | grep -qx 'browser_default=tor' || fail 'Tor Browser non choisi par défaut'
export HOME="$work/home"
manager --open 'https://example.invalid/tor'
grep -Fxq 'https://example.invalid/tor' "$work/arguments" || fail 'lien non ouvert dans Tor Browser'
copy="$HOME/.local/share/winlux/tor-browser"
test "$(cat "$copy/.winlux-version")" = 16.0.1 || fail 'copie personnelle de Tor Browser'
# Nouvelle version : copie refaite, profil gardé.
printf 'favoris\n' > "$copy/Browser/TorBrowser/Data/favoris"
printf '16.0.2\n' > "$installed/tor/version"
manager --launch tor 'https://example.invalid/tor2'
test "$(cat "$copy/.winlux-version")" = 16.0.2 || fail 'copie non refaite pour la nouvelle version'
test -f "$copy/Browser/TorBrowser/Data/favoris" || fail 'profil de Tor Browser perdu à la mise à jour'
echo 'OK : Brave, Opera GX, Vivaldi et Tor Browser (signature épinglée) installés et ouverts (#1716)'
