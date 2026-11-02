#!/bin/sh
# Explicit diagnostic boot in a disposable guest; never used by normal startup.
set -eu
ls -ld /dev /dev/null /home /home/winlux-web
/usr/bin/Xvfb :0 -ac -screen 0 1280x720x24 > /tmp/browser-xvfb.log 2>&1 &
xvfb=$!
trap 'kill "$xvfb" 2>/dev/null || true' EXIT HUP INT TERM
export DISPLAY=:0
sleep 2
kill -0 "$xvfb"
mkdir -p /home/winlux-web
printf '<!doctype html><title>WINLUX_BROWSER_OK</title><h1>Winlux</h1>\n' > /tmp/browser-test.html
chmod 644 /tmp/browser-test.html
# Aucun navigateur dans l'image : les trois sont proposés au téléchargement
# chez leur éditeur (pas de réseau dans cette VM), aucun n'est fourni.
status=$(winlux-browsers --status)
for browser in chrome edge firefox; do
    printf '%s\n' "$status" | grep -qx "browser_${browser}=downloadable"
    test ! -e "/usr/share/winlux/browsers/$browser.deb"
done
printf '%s\n' "$status" | grep -qx 'browser_default=none'
test ! -e /opt/microsoft && test ! -e /opt/google
# Sans réseau : échec propre, rien d'installé, pas de verrou.
if winlux-browsers --install firefox; then echo 'installation sans réseau ?' >&2; exit 1; fi
test ! -e /opt/winlux/browsers/firefox && test ! -e /run/winlux/browser-install.lock
for browser in edge chrome firefox; do echo "WINLUX_BROWSER_${browser}_RESULT=0"; done
