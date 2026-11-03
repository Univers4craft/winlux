#!/bin/sh
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
export WINLUX_BROWSER_ROOT="$work/system"
export WINLUX_BROWSER_TEST_LOG="$work/arguments"
cache="$WINLUX_BROWSER_ROOT/usr/share/winlux/browsers"
mkdir -p "$cache"
for id in edge chrome firefox; do
    package=$id
    case "$id" in
        edge) package=microsoft-edge-stable; binary=opt/microsoft/msedge/microsoft-edge ;;
        chrome) package=google-chrome-stable; binary=opt/google/chrome/google-chrome ;;
        firefox) binary=usr/lib/firefox/firefox ;;
    esac
    tree="$work/$id"
    mkdir -p "$tree/DEBIAN" "$tree/$(dirname "$binary")"
    printf 'Package: %s\nVersion: 1.0\nArchitecture: amd64\nMaintainer: test <test@example.invalid>\nDescription: browser fixture\n' "$package" > "$tree/DEBIAN/control"
    printf '#!/bin/sh\nprintf "%%s\\n" "$@" > "$WINLUX_BROWSER_TEST_LOG"\n' > "$tree/$binary"
    chmod 755 "$tree/$binary"
    dpkg-deb --build --root-owner-group "$tree" "$cache/$id.deb" >/dev/null
    sha256sum "$cache/$id.deb" | cut -d' ' -f1 > "$cache/$id.sha256"
    printf '1.0\n' > "$cache/$id.version"
done
# Table de routage simulée : en ligne (route par défaut) ou hors ligne.
printf 'Iface\tDestination\tGateway\neth0\t00000000\t0202000A\n' > "$work/routes-online"
printf 'Iface\tDestination\tGateway\neth0\t0002000A\t00000000\n' > "$work/routes-offline"
export WINLUX_NETWORK_ROUTES="$work/routes-online"
manager() {
    if command -v busybox >/dev/null 2>&1; then
        busybox sh "$project/scripts/winlux-browsers.sh" "$@"
    else
        sh "$project/scripts/winlux-browsers.sh" "$@"
    fi
}
manager --status | grep -qx 'browser_default=none'
manager --open 'https://example.invalid/'
test -f "$WINLUX_BROWSER_ROOT/run/winlux/browser-chooser"
umask 077
manager --default edge
test "$(stat -c %a "$WINLUX_BROWSER_ROOT/opt/winlux/browsers")" = 755
test "$(stat -c %a "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/edge")" = 755
test "$(stat -c %a "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/edge/root")" = 755
manager --status | grep -qx 'browser_edge=installed'
manager --launch edge 'https://example.invalid/?x=1&y=two words'
grep -Fxq 'https://example.invalid/?x=1&y=two words' "$work/arguments"
manager --default chrome
manager --open 'https://example.invalid/chrome'
grep -Fxq 'https://example.invalid/chrome' "$work/arguments"
cp "$cache/firefox.deb" "$work/firefox-valid.deb"
printf corrupt > "$cache/firefox.deb"
if manager --default firefox >/dev/null 2>&1; then echo 'Corrupt browser accepted' >&2; exit 1; fi
manager --status | grep -qx 'browser_default=chrome'
test ! -e "$WINLUX_BROWSER_ROOT/run/winlux/browser-install.lock"
cp "$work/firefox-valid.deb" "$cache/firefox.deb"
# Paquets de langue de Firefox : installés avec lui (fr), un paquet altéré (de) ignoré.
for lang in fr de; do
    pack="$work/l10n-$lang"
    mkdir -p "$pack/DEBIAN" "$pack/usr/lib/firefox/distribution/extensions"
    chmod -R 755 "$pack"
    printf '%s' "$lang" > "$pack/usr/lib/firefox/distribution/extensions/langpack-$lang@firefox.mozilla.org.xpi"
    printf 'Package: firefox-l10n-%s\nVersion: 1.0\nArchitecture: all\nMaintainer: test <test@example.invalid>\nDescription: langpack fixture\n' "$lang" > "$pack/DEBIAN/control"
    dpkg-deb --build --root-owner-group "$pack" "$cache/firefox-l10n-$lang.deb" >/dev/null
    sha256sum "$cache/firefox-l10n-$lang.deb" | cut -d' ' -f1 > "$cache/firefox-l10n-$lang.sha256"
done
printf '%064d\n' 0 > "$cache/firefox-l10n-de.sha256"
manager --default firefox 2>"$work/langpacks.txt"
extensions="$WINLUX_BROWSER_ROOT/opt/winlux/browsers/firefox/root/usr/lib/firefox/distribution/extensions"
test -f "$extensions/langpack-fr@firefox.mozilla.org.xpi" || { echo 'Firefox sans paquet de langue' >&2; exit 1; }
test ! -e "$extensions/langpack-de@firefox.mozilla.org.xpi" || { echo 'Paquet de langue altéré installé' >&2; exit 1; }
grep -q 'Paquet de langue altéré ignoré : de' "$work/langpacks.txt"
grep -qx 'pref("intl.locale.requested", "");' "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/firefox/root/usr/lib/firefox/defaults/pref/winlux-langue.js"
touch "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/firefox/marker"
manager --install firefox
test -f "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/firefox/marker"
if manager --install unknown >/dev/null 2>&1; then exit 1; fi
# Edge et Chrome absents de l'image : téléchargés chez l'éditeur, à la demande.
rm -f "$cache/chrome.deb" "$cache/chrome.sha256" "$cache/chrome.version"
rm -rf "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/chrome"
dpkg-deb --build --root-owner-group "$work/chrome" "$work/chrome-publisher.deb" >/dev/null
cat > "$work/fetch" <<FETCH
#!/bin/sh
set -eu
test "\$1" = --only && test "\$2" = chrome
printf '%s\n' "\$*" > "$work/fetch-arguments"
cp "$work/chrome-publisher.deb" "\$3/chrome.deb"
sha256sum "\$3/chrome.deb" | cut -d' ' -f1 > "\$3/chrome.sha256"
printf '2.0\n' > "\$3/chrome.version"
FETCH
chmod 755 "$work/fetch"
export WINLUX_BROWSER_FETCH="$work/fetch"
manager --status | grep -qx 'browser_chrome=downloadable'
manager --install chrome
grep -q "^--only chrome $WINLUX_BROWSER_ROOT/var/cache/winlux/browsers\$" "$work/fetch-arguments"
manager --status | grep -qx 'browser_chrome=installed'
test ! -e "$cache/chrome.deb"
# Échec du téléchargement : rien d'installé, pas de verrou orphelin.
cp "$cache/edge.deb" "$work/edge.deb.saved"
rm -rf "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/edge" "$cache/edge.deb" "$cache/edge.sha256" "$cache/edge.version"
printf '#!/bin/sh\nexit 1\n' > "$work/fetch"
if manager --install edge >/dev/null 2>&1; then echo 'Edge installé sans téléchargement' >&2; exit 1; fi
test ! -e "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/edge"
test ! -e "$WINLUX_BROWSER_ROOT/run/winlux/browser-install.lock"
# Hors ligne : message clair (code 75), aucune trace technique, reprise notée.
WINLUX_NETWORK_ROUTES="$work/routes-offline"
code=0; manager --install edge 2>"$work/offline.txt" || code=$?
test "$code" = 75
grep -q "Pas de connexion Internet" "$work/offline.txt"
if grep -qi "traceback\|urlopen\|errno" "$work/offline.txt"; then echo 'Trace technique affichée hors ligne' >&2; exit 1; fi
grep -qx edge "$WINLUX_BROWSER_ROOT/etc/winlux/browser-install-pending"
code=0; manager --default edge 2>/dev/null || code=$?
test "$code" = 75
test "$(cat "$WINLUX_BROWSER_ROOT/etc/winlux/first-boot-browser")" = edge
# Le réseau tombe pendant le téléchargement : même message, journal à part.
printf '#!/bin/sh\necho "Traceback: urlopen error" >&2\nexit 1\n' > "$work/fetch"
WINLUX_NETWORK_ROUTES="$work/routes-online"
code=0; manager --install edge 2>"$work/failed.txt" || code=$?
test "$code" = 1
if grep -q Traceback "$work/failed.txt"; then echo 'Trace technique affichée' >&2; exit 1; fi
grep -q Traceback "$WINLUX_BROWSER_ROOT/run/winlux/browser-fetch.log"
# Horloge fausse (pile vide, 2015) : le vrai motif, pas « ne répond pas ».
printf '#!/bin/sh\necho "curl: (60) SSL certificate problem: certificate is not yet valid" >&2\nexit 1\n' > "$work/fetch"
code=0; manager --install edge 2>"$work/clock.txt" || code=$?
test "$code" = 1
grep -q "La date de ce PC est fausse" "$work/clock.txt"
grep -q "Paramètres > Heure et langue" "$work/clock.txt"
if grep -q "ne répond pas" "$work/clock.txt"; then echo 'Date fausse annoncée comme site muet' >&2; exit 1; fi
# Retour du réseau : installé, plus rien en attente.
sed 's/chrome/edge/g' > "$work/fetch" <<FETCH
#!/bin/sh
set -eu
cp "$work/edge-publisher.deb" "\$3/chrome.deb"
sha256sum "\$3/chrome.deb" | cut -d' ' -f1 > "\$3/chrome.sha256"
printf '2.0\n' > "\$3/chrome.version"
FETCH
cp "$work/edge.deb.saved" "$work/edge-publisher.deb"
manager --default edge
test ! -e "$WINLUX_BROWSER_ROOT/etc/winlux/browser-install-pending"
test ! -e "$WINLUX_BROWSER_ROOT/etc/winlux/first-boot-browser"
manager --status | grep -qx 'browser_edge=installed'
# Paramètres > Navigateurs : vrais noms, « Par défaut »/« Ouvrir » pour les seuls
# navigateurs installés, « Installer » pour les autres. Logique sans GTK.
manager --status > "$work/status.txt"
python3 -B - "$project/scripts/winlux-settings.py" "$work/status.txt" <<'PY'
import ast, sys
tree = ast.parse(open(sys.argv[1], encoding="utf-8").read())
keep = [n for n in tree.body if (isinstance(n, ast.Assign) and any(getattr(t, "id", "") == "BROWSER_NAMES" for t in n.targets))
        or (isinstance(n, ast.FunctionDef) and n.name in ("browser_buttons_shown", "browser_default_text"))]
scope = {"_t": lambda text: text}
exec(compile(ast.Module(body=keep, type_ignores=[]), "settings", "exec"), scope)
names, shown = scope["BROWSER_NAMES"], scope["browser_buttons_shown"]
assert names == {"chrome": "Google Chrome", "edge": "Microsoft Edge", "firefox": "Mozilla Firefox"}, names
status = dict(l.split("=", 1) for l in open(sys.argv[2]).read().split() if "=" in l)
def visible(status):
    return sorted(f"{v}-{i}" for i in names for v in ("install", "default", "launch") if shown(status, f"browser-{v}-{i}"))
# État réel ici : les trois sont installés, Edge par défaut (pas de
# « Définir par défaut » pour lui : c'est déjà le cas).
assert status["browser_default"] == "edge", status
assert visible(status) == ["default-chrome", "default-firefox"] + [f"launch-{i}" for i in names], (status, visible(status))
text = scope["browser_default_text"]
assert text(status) == "Microsoft Edge"
assert text({"browser_default": "none"}) == "Aucun : choisissez-en un ci-dessous"
# Un seul navigateur installé, aucun défaut : c'est lui qui ouvre les liens.
assert text({"browser_default": "none", "browser_firefox": "installed"}) == "Mozilla Firefox (seul navigateur installé)"
assert text({"browser_default": "none", "browser_firefox": "installed", "browser_chrome": "update"}).startswith("Aucun : le navigateur vous sera demandé")
status["browser_default"] = "none"
status.update(browser_chrome="downloadable", browser_firefox="update")
assert visible(status) == ["default-edge", "default-firefox", "install-chrome", "install-firefox",
                           "launch-edge", "launch-firefox"], visible(status)
assert visible({}) == [f"install-{i}" for i in names]
source = open(sys.argv[1], encoding="utf-8").read()
assert 'format(name=BROWSER_NAMES[ident])' in source and 'for name in ("chrome", "edge", "firefox")' not in source
# Bouton d'action, pas d'état ; lien gardé ouvert dès l'installation finie.
assert '("Définir {name} par défaut", "default")' in source and '"Par défaut : {name}"' not in source
assert 'action.startswith(("browser-install-", "browser-default-")):' in source and "open_pending_link()" in source
assert '["/usr/bin/winlux-browser", "--pending"]' in source
PY
if python3 -B "$project/scripts/prepare-browser-packages.py" --only safari "$work/none" >/dev/null 2>&1; then exit 1; fi
python3 -B "$project/scripts/test-browser-downloads.py"
echo 'OK: no browser shipped or preferred, choice screen, download on demand, URL arguments, checksums and cleanup'
