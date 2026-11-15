#!/bin/sh
# #2243 : sans navigateur, la demande « ouvrir Paramètres > Navigateurs » est
# propre au compte. Un nom commun sous /run/winlux (sticky) appartenait au
# premier compte qui l'écrivait : les autres recevaient « Permission denied »
# et seule la session principale l'ouvrait.
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() { echo "ÉCHEC : $*" >&2; exit 1; }
export WINLUX_BROWSER_ROOT="$work/system"
export WINLUX_BROWSER_FETCH="$work/absent"
run="$WINLUX_BROWSER_ROOT/run/winlux"
mkdir -p "$run"
chmod 1777 "$run"
me=$(id -u)
other=$((me + 1))

# Demande d'un autre compte déjà présente, non modifiable : elle ne gêne pas.
printf '1\n' > "$run/browser-chooser-$other"
chmod 444 "$run/browser-chooser-$other"
sh "$project/scripts/winlux-browsers.sh" --open 'https://example.invalid/' 2>"$work/err" \
    || fail "winlux-browsers --open échoue : $(cat "$work/err")"
test -f "$run/browser-chooser-$me" || fail "aucune demande pour le compte $me"
test ! -e "$run/browser-chooser" || fail 'nom commun sans uid encore écrit'
grep -q 'Permission denied' "$work/err" && fail "refus d'écriture : $(cat "$work/err")"

# Une demande au nom du compte qui ne peut pas être écrite ne fait pas échouer le lanceur.
rm -f "$run/browser-chooser-$me"
mkdir "$run/browser-chooser-$me"
sh "$project/scripts/winlux-browsers.sh" --open 'https://example.invalid/' 2>/dev/null \
    || fail 'le lanceur s’arrête quand la demande ne peut pas être écrite'
rmdir "$run/browser-chooser-$me"

# Chaque session traite sa propre demande, principale ou non.
for session in winlux-user-session.sh winlux-native-session.sh; do
    grep -q 'browser-chooser-\$' "$project/scripts/$session" || fail "$session lit encore un nom commun"
done
grep -q 'test "$primary" = 1 && test -O /run/winlux/browser-chooser' "$project/scripts/winlux-user-session.sh" \
    && fail 'seule la session principale ouvre encore le choix du navigateur'
echo "OK : choix du navigateur demandé par compte, sans refus entre comptes (#2243)"
