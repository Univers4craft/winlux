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
manager --status | grep -qx 'browser_default=edge'
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
python3 -B "$project/scripts/test-browser-downloads.py"
echo 'OK: offline browser selection, installation, default, URL arguments, checksums and cleanup'
