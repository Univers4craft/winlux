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
manager --default firefox
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
rm -rf "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/edge" "$cache/edge.deb" "$cache/edge.sha256" "$cache/edge.version"
printf '#!/bin/sh\nexit 1\n' > "$work/fetch"
if manager --install edge >/dev/null 2>&1; then echo 'Edge installé sans téléchargement' >&2; exit 1; fi
test ! -e "$WINLUX_BROWSER_ROOT/opt/winlux/browsers/edge"
test ! -e "$WINLUX_BROWSER_ROOT/run/winlux/browser-install.lock"
if python3 -B "$project/scripts/prepare-browser-packages.py" --only safari "$work/none" >/dev/null 2>&1; then exit 1; fi
python3 -B "$project/scripts/test-browser-downloads.py"
echo 'OK: no browser shipped or preferred, choice screen, download on demand, URL arguments, checksums and cleanup'
