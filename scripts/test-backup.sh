#!/bin/sh
# Historique des fichiers : instantanés incrémentaux, versions, restauration
# et conservation, dans une racine temporaire (sans disque ni partage réel).
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-backup-test.XXXXXX")
trap 'test -n "${KEEP:-}" || rm -rf "$work"' EXIT HUP INT TERM
root="$work/root"
mkdir -p "$root/etc" "$root/proc" "$root/home/alice/Documents/Projet" "$root/home/alice/Images" "$root/media/usb"
printf 'alice:x:1000:1000::/home/alice:/bin/sh\n' > "$root/etc/passwd"
: > "$root/proc/mounts"
export WINLUX_CONTROL_ROOT="$root"
backup() { sh "$project/scripts/winlux-backup.sh" "$@"; }
check() { grep -q "$1" "$2" || { echo "attendu « $1 » dans $2" >&2; cat "$2" >&2; exit 1; }; }
status="$root/run/winlux/backup/alice/status.txt"

for rsync_mode in rsync portable; do
    if test "$rsync_mode" = portable; then export WINLUX_BACKUP_NO_RSYNC=1; elif ! command -v rsync >/dev/null 2>&1; then continue; fi
    rm -rf "$root/etc/winlux" "$root/run" "$root/media/usb/WinluxFileHistory"
    printf 'v1\n' > "$root/home/alice/Documents/rapport.txt"
    printf 'stable\n' > "$root/home/alice/Documents/Projet/notes avec espace.txt"
    touch -d '2026-01-01 10:00' "$root/home/alice/Documents/rapport.txt" "$root/home/alice/Documents/Projet/notes avec espace.txt"

    backup status alice
    check '^enabled=0$' "$status"
    check '^folder=1|/home/alice/Documents$' "$status"
    check '^folder=0|/home/alice/Bureau$' "$status"

    # Validation des réglages.
    if backup set alice FREQUENCY 7 2>/dev/null; then echo 'fréquence invalide acceptée' >&2; exit 1; fi
    if backup set alice DESTINATION /absent 2>/dev/null; then echo 'destination absente acceptée' >&2; exit 1; fi
    if backup status '../etc' 2>/dev/null; then echo 'utilisateur invalide accepté' >&2; exit 1; fi
    backup set alice DESTINATION /media/usb
    backup set alice ENABLED 1
    backup set alice FREQUENCY 10
    check '^available=1$' "$status"

    # Premier instantané puis instantané incrémental.
    backup run alice > "$work/run1.log"
    check '^last_result=ok$' "$status"
    history="$root/media/usb/WinluxFileHistory/$(cat "$root/etc/hostname" 2>/dev/null || hostname 2>/dev/null || echo winlux)/alice"
    first=$(ls "$history" | head -n 1)
    test -f "$history/$first/home/alice/Documents/rapport.txt"
    sleep 1
    printf 'v2\n' > "$root/home/alice/Documents/rapport.txt"
    backup run alice > "$work/run2.log"
    second=$(ls "$history" | tail -n 1)
    test "$first" != "$second"
    # Inchangé : même inode (lien dur). Modifié : copie distincte.
    a=$(stat -c %i "$history/$first/home/alice/Documents/Projet/notes avec espace.txt")
    b=$(stat -c %i "$history/$second/home/alice/Documents/Projet/notes avec espace.txt")
    test "$a" = "$b" || { echo "fichier inchangé recopié ($rsync_mode)" >&2; exit 1; }
    test "$(cat "$history/$first/home/alice/Documents/rapport.txt")" = v1
    test "$(cat "$history/$second/home/alice/Documents/rapport.txt")" = v2

    # Liste, navigation et versions.
    backup list alice
    test "$(wc -l < "$root/run/winlux/backup/alice/snapshots.txt")" -eq 2
    backup browse alice "$second" /
    check '^entry=d|/home/alice/Documents|' "$root/run/winlux/backup/alice/browse.txt"
    backup browse alice "$second" /home/alice/Documents
    check '^entry=f|rapport.txt|3|' "$root/run/winlux/backup/alice/browse.txt"
    check '^entry=d|Projet|' "$root/run/winlux/backup/alice/browse.txt"
    backup versions alice /home/alice/Documents/rapport.txt
    test "$(grep -c '^version=' "$root/run/winlux/backup/alice/versions.txt")" -eq 2
    backup versions alice "/home/alice/Documents/Projet/notes avec espace.txt"
    test "$(grep -c '^version=' "$root/run/winlux/backup/alice/versions.txt")" -eq 1

    # Restauration : remplacer, garder les deux, dossier complet, via requête.
    backup restore alice "$first" /home/alice/Documents/rapport.txt keep-both
    test "$(cat "$root/home/alice/Documents/rapport (restauré $first).txt")" = v1
    test "$(cat "$root/home/alice/Documents/rapport.txt")" = v2
    backup restore alice "$first" /home/alice/Documents/rapport.txt
    test "$(cat "$root/home/alice/Documents/rapport.txt")" = v1
    rm -rf "$root/home/alice/Documents/Projet"
    printf 'restore\n%s\n%s\n' "$second" /home/alice/Documents/Projet > "$root/run/winlux/backup/alice/request"
    backup request alice
    test "$(cat "$root/home/alice/Documents/Projet/notes avec espace.txt")" = stable
    if backup restore alice "$first" /etc/passwd 2>/dev/null; then echo 'restauration hors dossiers acceptée' >&2; exit 1; fi
    if backup browse alice "$first" /home/alice/../../etc 2>/dev/null; then echo 'chemin .. accepté' >&2; exit 1; fi

    # Ajout et retrait de dossiers protégés via requête (espaces et accents).
    printf 'add\n/home/alice/Mes Vidéos\n' > "$root/run/winlux/backup/alice/request"
    backup request alice
    check '^folder=0|/home/alice/Mes Vidéos$' "$status"
    backup remove alice "/home/alice/Mes Vidéos"
    if grep -q 'Mes Vidéos' "$status"; then echo 'dossier non retiré' >&2; exit 1; fi

    # Planification : auto-run ne relance pas avant l'échéance.
    count=$(ls "$history" | wc -l)
    backup auto-run
    test "$(ls "$history" | wc -l)" -eq "$count"
    sed -i 's/^LAST_ATTEMPT=.*/LAST_ATTEMPT=1/' "$root/etc/winlux/backup/alice.conf"
    sleep 1
    backup auto-run
    test "$(ls "$history" | wc -l)" -eq $((count + 1))

    # Conservation : un instantané ancien expire, le plus récent reste.
    sed -i 's/^epoch=.*/epoch=1000/' "$history/$first/.winlux-snapshot"
    mkdir -p "$history/.20000101-000000.partial"
    backup set alice KEEP_DAYS 30
    backup prune alice
    test ! -d "$history/$first"
    test ! -d "$history/.20000101-000000.partial"
    test -d "$history/$second"

    # Disque débranché : échec clair, aucun instantané créé.
    mv "$root/media/usb" "$root/media/usb-off"
    if backup run alice 2>/dev/null; then echo 'sauvegarde sans disque acceptée' >&2; exit 1; fi
    check '^last_result=unavailable$' "$status"
    check '^available=0$' "$status"
    mv "$root/media/usb-off" "$root/media/usb"
done
echo "Historique des fichiers : OK"
