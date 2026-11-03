#!/bin/sh
# Liens web (xdg-open, $BROWSER, ShellExecute des .exe) : ils s'ouvrent dans le
# navigateur installé, même s'il n'a pas été choisi « par défaut ». Le choix du
# navigateur n'apparaît que s'il n'y en a aucun, et le lien demandé s'ouvre
# ensuite au lieu d'être perdu.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
export WINLUX_BROWSER_ROOT="$work/system"
log="$work/arguments"
run="$WINLUX_BROWSER_ROOT/run/winlux"
pending="$run/browser-pending-$(id -u)"
installed="$WINLUX_BROWSER_ROOT/opt/winlux/browsers"
mkdir -p "$work/bin" "$run"
printf '#!/bin/sh\nexec sh "%s/scripts/winlux-browsers.sh" "$@"\n' "$project" > "$work/bin/winlux-browsers"
chmod 755 "$work/bin/winlux-browsers"
PATH="$work/bin:$PATH"
export PATH
# Aucun navigateur du système (les machines de CI en ont).
export WINLUX_BROWSER_CANDIDATES=
unset WINLUX_BROWSER || true
fixture() {
    printf '#!/bin/sh\nprintf "%%s\\n" "%s" "$@" > "%s"\n' "$1" "$log" > "$2"
    chmod 755 "$2"
}
catalogue() {
    case "$1" in
        chrome) binary=chrome/root/opt/google/chrome/google-chrome ;;
        edge) binary=edge/root/opt/microsoft/msedge/microsoft-edge ;;
        firefox) binary=firefox/root/usr/lib/firefox/firefox ;;
    esac
    mkdir -p "$(dirname "$installed/$binary")"
    fixture "$1" "$installed/$binary"
}
browser() { sh "$project/integration/winlux-browser" "$@"; }
fail() { echo "ÉCHEC : $*" >&2; exit 1; }

# 1. Aucun navigateur : le choix s'ouvre et le lien est gardé.
browser 'https://example.invalid/connexion?code=1&state=a b' 2>/dev/null || true
test -f "$run/browser-chooser" || fail "le choix du navigateur ne s'ouvre pas sans navigateur"
grep -Fxq 'https://example.invalid/connexion?code=1&state=a b' "$pending" || fail 'lien perdu pendant le choix'
# Toujours aucun navigateur : le lien attend.
browser --pending
test -f "$pending" || fail 'lien abandonné avant le choix'
test ! -e "$log" || fail 'navigateur lancé sans navigateur'

# 2. Firefox installé sans être « par défaut » (bouton Installer, ou reprise au
#    démarrage connecté) : le lien s'ouvre dedans, sans passer par les Paramètres.
rm -f "$run/browser-chooser"
catalogue firefox
sh "$project/scripts/winlux-browsers.sh" --status | grep -qx 'browser_default=none'
browser 'https://example.invalid/apres'
test "$(sed -n 1p "$log")" = firefox || fail "navigateur installé ignoré : $(cat "$log")"
grep -Fxq 'https://example.invalid/apres' "$log"
test ! -e "$run/browser-chooser" || fail 'Paramètres ouverts alors qu’un navigateur est installé'

# 3. Le lien gardé s'ouvre dès que le navigateur est là, une seule fois.
rm -f "$log"
browser --pending
grep -Fxq 'https://example.invalid/connexion?code=1&state=a b' "$log" || fail 'lien gardé non ouvert après le choix'
test ! -e "$pending" || fail 'lien gardé rouvert à chaque passage'
rm -f "$log"
browser --pending
test ! -e "$log" || fail 'navigateur relancé sans lien en attente'

# 4. Le navigateur par défaut passe avant les autres installés.
catalogue chrome
catalogue edge
mkdir -p "$WINLUX_BROWSER_ROOT/etc/winlux"
printf 'default=edge\n' > "$WINLUX_BROWSER_ROOT/etc/winlux/browser.conf"
browser 'https://example.invalid/defaut'
test "$(sed -n 1p "$log")" = edge || fail "défaut ignoré : $(cat "$log")"
# Défaut désinstallé : le premier installé, par ordre alphabétique (aucun favori).
rm -rf "${installed:?}/edge"
browser 'https://example.invalid/sans-defaut'
test "$(sed -n 1p "$log")" = chrome || fail "repli : $(cat "$log")"

# 5. Navigateur du système seulement (hors catalogue).
rm -rf "${installed:?}"/*
fixture system "$work/bin/autre-navigateur"
WINLUX_BROWSER_CANDIDATES=autre-navigateur browser 'https://example.invalid/systeme'
test "$(sed -n 1p "$log")" = system || fail "navigateur du système ignoré : $(cat "$log")"
grep -Fxq 'https://example.invalid/systeme' "$log"
test ! -e "$run/browser-chooser"

# 6. Un lien resté en attente plus d'une heure n'est plus ouvert.
printf 'https://example.invalid/ancien\n' > "$pending"
touch -d '2 hours ago' "$pending"
rm -f "$log"
WINLUX_BROWSER_CANDIDATES=autre-navigateur browser --pending
test ! -e "$pending" && test ! -e "$log" || fail 'lien périmé ouvert'

# 7. Tous les chemins mènent à winlux-browser : xdg-open (GIO et exo-open
#    d'XFCE), $BROWSER, ShellExecute des .exe ; la session ouvre le lien gardé.
grep -q 'x-scheme-handler/https=winlux-browser.desktop' "$project/scripts/winlux-native-config.py"
grep -q '^MimeType=x-scheme-handler/http;x-scheme-handler/https;' "$project/integration/winlux-browser.desktop"
grep -qx 'WebBrowser=winlux-browser' "$project/integration/native/config/xfce4/helpers.rc"
grep -q '^X-XFCE-CommandsWithParameter=/usr/bin/winlux-browser "%s";' "$project/integration/native/helpers/winlux-browser.desktop"
python3 -B - "$project/runtime/src/dlls/shell32.rs" <<'PY'
import re, sys
source = open(sys.argv[1], encoding="utf-8").read()
body = source[source.index("pub fn browser_candidates"):]
names = re.findall(r'"([a-z-]+)"', body[:body.index("list\n}")])
assert names[0] == "winlux-browser", names
PY
for session in winlux-native-session.sh winlux-user-session.sh; do
    grep -q 'export BROWSER="${BROWSER:-/usr/bin/winlux-browser}"' "$project/scripts/$session" || fail "$session : BROWSER"
    grep -q '/usr/bin/winlux-browser --pending' "$project/scripts/$session" || fail "$session : lien gardé"
done
echo 'OK: liens web ouverts dans le navigateur installé, choix seulement sans navigateur, lien gardé puis ouvert'
