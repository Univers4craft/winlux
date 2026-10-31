#!/bin/sh
# Partage de dossiers (winlux-share) : registre, droits par compte, refus des
# dossiers système, configuration Samba générée, pare-feu et désactivation,
# dans une racine temporaire. Si Samba est installé, la configuration est
# aussi validée par testparm ; le serveur n'est pas démarré.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-share-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
root="$work/root"
export WINLUX_CONTROL_ROOT="$root" WINLUX_SHARE_AVAILABLE=1 WINLUX_SHARE_NO_SERVER=1 HOME="$root/home/alice"
share() { sh "$project/scripts/winlux-share.sh" "$@" >/dev/null; }
state="$root/run/winlux/shares.txt"
conf="$root/etc/winlux/samba/smb.conf"
fail() { echo "ÉCHEC : $*" >&2; cat "$state" "$conf" >&2 2>/dev/null || true; exit 1; }
mkdir -p "$root/etc/winlux" "$HOME/Photos été" "$HOME/Musique"
printf 'root:root:admin\nalice:user:s3cret\nbob:user:nopass\n' > "$root/etc/winlux/users.conf"

share status
grep -qx 'available=1' "$state" || fail "disponibilité non publiée"
grep -qx 'account=bob|0' "$state" || fail "compte sans mot de passe non signalé"
! grep -q '^share=' "$state" || fail "partage présent par défaut"

share add "$HOME/Photos été"
grep -q '^share=1|Photos été|C:\\Photos été|none|' "$state" || fail "partage non ajouté"
share add "$HOME/Photos été"
test "$(grep -c '^share=' "$state")" = 1 || fail "le même dossier est partagé deux fois"
mkdir -p "$root/srv/Photos été"
share add "$root/srv/Photos été"
grep -q '^share=2|Photos été 2|' "$state" || fail "nom en double non renommé"
share remove 2

for bad in /etc /usr/share /proc /; do
    mkdir -p "$root$bad" 2>/dev/null || true
    if share add "$root$bad" 2>/dev/null; then fail "dossier système partagé : $bad"; fi
done
if share set 1 mallory write 2>/dev/null; then fail "compte inconnu accepté"; fi
grep -qx 'error=compte inconnu : mallory' "$state" || fail "erreur non publiée pour l'interface"
share status
! grep -q '^error=' "$state" || fail "erreur publiée deux fois"
if share set 1 alice admin 2>/dev/null; then fail "droit inconnu accepté"; fi
if share set 9 alice read 2>/dev/null; then fail "partage inconnu accepté"; fi

share set 1 alice write
share set 1 bob none
grep -q '^share=1|.*|none|.*alice:write' "$state" || fail "droit d'écriture non enregistré"
grep -q '^\[Photos été\]$' "$conf" || fail "section Samba absente"
users=$(sed -n 's/^   valid users = //p' "$conf" | tr ' ' '\n')
printf '%s\n' "$users" | grep -qx alice || fail "alice non autorisée"
! printf '%s\n' "$users" | grep -qx bob || fail "bob encore autorisé"
sed -n 's/^   write list = //p' "$conf" | tr ' ' '\n' | grep -qx alice || fail "liste d'écriture incorrecte"
grep -qx '   guest ok = no' "$conf" || fail "accès invité ouvert par défaut"
grep -qx '   server min protocol = SMB2_10' "$conf" || fail "SMB1 non désactivé"
grep -q '^rule=9445	.*	445,5357		private,domain	1$' "$root/etc/winlux/firewall.conf" || fail "règle de pare-feu absente"

share guest 1 read
grep -qx '   guest ok = yes' "$conf" || fail "accès invité non ouvert"
grep -qx '   map to guest = bad user' "$conf" || fail "compte invité non configuré"
if command -v testparm >/dev/null 2>&1; then
    testparm -s "$conf" >/dev/null 2>&1 || fail "configuration Samba refusée par testparm"
fi

share disable
grep -qx 'enabled=0' "$state" || fail "désactivation non publiée"
! grep -q '^\[Photos' "$conf" || fail "partage encore servi après désactivation"
! grep -q '^rule=9445' "$root/etc/winlux/firewall.conf" || fail "pare-feu encore ouvert après désactivation"
share enable
grep -q '^\[Photos été\]$' "$conf" || fail "réactivation sans effet"

export WINLUX_SHARE_AVAILABLE=0 PATH=/usr/bin:/bin
if ! command -v smbd >/dev/null 2>&1 && ! test -x /usr/sbin/smbd; then
    share status
    grep -qx 'available=0' "$state" || fail "Samba absent mais partage proposé"
fi
echo "Partage de dossiers : OK"
