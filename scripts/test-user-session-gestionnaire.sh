#!/bin/sh
# #3401 : la session d'un second compte attend son gestionnaire de fenêtres,
# et s'arrête en le disant s'il est mort ou vivant mais muet, au lieu de
# s'ouvrir sans lui. La fonction attendre_gestionnaire de
# winlux-user-session.sh est chargée seule et pilotée par un faux wmctrl.
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d); muet=
trap 'kill "$muet" 2>/dev/null || true; rm -rf "$work"' EXIT
fail() { echo "ÉCHEC : $*" >&2; exit 1; }
sed -n '/^attendre_gestionnaire() {$/,/^}$/p' "$project/scripts/winlux-user-session.sh" > "$work/fonction.sh"
grep -q 'return 1' "$work/fonction.sh" || fail "attendre_gestionnaire introuvable dans winlux-user-session.sh"
grep -qx 'attendre_gestionnaire || exit 1' "$project/scripts/winlux-user-session.sh" \
    || fail "la session n'appelle pas attendre_gestionnaire, ou continue quand il échoue"
logs=$work; wm=faux-openbox
printf 'journal du gestionnaire\n' > "$logs/window-manager.log"
cat > "$work/repond-au-3e" <<'S'
#!/bin/sh
n=$(cat "$0.n" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$0.n"
test "$n" -ge 3
S
printf '#!/bin/sh\nexit 1\n' > "$work/muet-wmctrl"
chmod +x "$work/repond-au-3e" "$work/muet-wmctrl"
sleep 60 & muet=$!
. "$work/fonction.sh"

# Gestionnaire qui répond après quelques essais : la session continue.
wmctrl=$work/repond-au-3e wm_pid=$muet WINLUX_WM_WAIT_TENTHS=50
attendre_gestionnaire 2>"$work/err" || fail "gestionnaire qui répond refusé : $(cat "$work/err")"
test ! -s "$work/err" || fail "message inattendu : $(cat "$work/err")"

# Vivant mais muet : échec, message clair, journal, gestionnaire arrêté.
wmctrl=$work/muet-wmctrl WINLUX_WM_WAIT_TENTHS=5
debut=$(date +%s)
if attendre_gestionnaire 2>"$work/err"; then fail "un gestionnaire muet laisse la session continuer"; fi
grep -q "vivant mais n'a pas répondu" "$work/err" || fail "message absent : $(cat "$work/err")"
grep -q "journal du gestionnaire" "$work/err" || fail "journal du gestionnaire absent"
test $(( $(date +%s) - debut )) -le 5 || fail "l'attente dépasse sa limite"
sleep 0.2
! kill -0 "$muet" 2>/dev/null || fail "le gestionnaire muet n'a pas été arrêté"

# Mort au démarrage : échec immédiat, avec son journal.
wm_pid=$muet WINLUX_WM_WAIT_TENTHS=300
debut=$(date +%s)
if attendre_gestionnaire 2>"$work/err"; then fail "un gestionnaire mort laisse la session continuer"; fi
grep -q "s'est arrêté au démarrage" "$work/err" || fail "message absent : $(cat "$work/err")"
test $(( $(date +%s) - debut )) -le 2 || fail "un gestionnaire mort fait attendre"
# L'attente par défaut reste sous le seuil de 20 s de l'init pour un échec
# rapide de session : sinon un gestionnaire muet relancerait la session sans
# fin, sans jamais mener à l'écran de secours.
defaut=$(sed -n 's/.*WINLUX_WM_WAIT_TENTHS:-\([0-9]*\).*/\1/p' "$work/fonction.sh")
seuil=$(sed -n 's/.*session_started )) -lt \([0-9]*\); then/\1/p' "$project/scripts/build-live-iso.sh" | head -n 1)
test -n "$defaut" && test -n "$seuil" || fail "attente par défaut ($defaut) ou seuil de l'init ($seuil) introuvable"
test $((defaut / 10 + 2)) -lt "$seuil" || fail "attente de $((defaut / 10)) s trop proche du seuil d'échec rapide de l'init ($seuil s)"
echo "test-user-session-gestionnaire : ok"
