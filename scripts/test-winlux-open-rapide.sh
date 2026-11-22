#!/bin/sh
# winlux-open ne retarde pas le programme (#2889) :
#  - le programme est lancé sans attendre `winlux-applications register`
#    (ici un faux qui dort 3 s), et l'enregistrement a quand même lieu ;
#  - un programme lançable n'est inspecté qu'une fois (`--inspect`), la
#    seconde inspection ne sert qu'à expliquer un refus.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
open="$root/integration/winlux-open"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() { echo "ÉCHEC : $1" >&2; exit 1; }
mkdir -p "$work/bin" "$work/etat"
# Faux runtime : compte les inspections, note l'instant du lancement.
cat >"$work/winlux" <<FIN
#!/bin/sh
if [ "\$1" = --inspect ]; then
    echo x >>"$work/inspections"
    case "\$2" in *refus.exe) echo "format inconnu" >&2; exit 1 ;; esac
    echo gui; exit 0
fi
date +%s%N >"$work/lance"
exit 0
FIN
# Faux winlux-applications : un enregistrement lent.
cat >"$work/bin/winlux-applications" <<FIN
#!/bin/sh
[ "\$1" = register ] || exit 0
sleep 3
printf '%s\n' "\$2" >"$work/enregistre"
FIN
chmod +x "$work/winlux" "$work/bin/winlux-applications"
printf 'MZ' >"$work/prog.exe"
printf 'MZ' >"$work/refus.exe"

debut=$(date +%s%N)
PATH="$work/bin:$PATH" WINLUX_RUNTIME="$work/winlux" XDG_STATE_HOME="$work/etat" \
    DISPLAY= WAYLAND_DISPLAY= sh "$open" "$work/prog.exe" </dev/null >/dev/null 2>&1 \
    || fail "winlux-open a échoué"
test -s "$work/lance" || fail "le programme n'a pas été lancé"
attente=$(( ($(cat "$work/lance") - debut) / 1000000 ))
[ "$attente" -lt 1500 ] || fail "programme lancé après $attente ms : l'enregistrement est attendu"
n=$(wc -l <"$work/inspections")
[ "$n" -eq 1 ] || fail "$n inspections pour un programme lançable, 1 attendue"
# L'enregistrement se termine en tâche de fond.
i=0
until [ -s "$work/enregistre" ]; do
    i=$((i + 1)); [ "$i" -le 100 ] || fail "programme jamais enregistré pour le menu Démarrer"
    sleep 0.1
done
[ "$(cat "$work/enregistre")" = "$work/prog.exe" ] || fail "mauvais chemin enregistré"
# Un refus reste expliqué (seconde inspection pour la raison).
rm -f "$work/inspections"
PATH="$work/bin:$PATH" WINLUX_RUNTIME="$work/winlux" XDG_STATE_HOME="$work/etat" \
    DISPLAY= WAYLAND_DISPLAY= sh "$open" "$work/refus.exe" </dev/null >"$work/refus.log" 2>&1 && fail "refus accepté"
grep -q "format inconnu" "$work/refus.log" || fail "raison du refus absente : $(cat "$work/refus.log")"
echo "winlux-open : lancé en $attente ms malgré un enregistrement de 3 s, une seule inspection (#2889)"
