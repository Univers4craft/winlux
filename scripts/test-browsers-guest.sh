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
for browser in edge chrome firefox; do
    winlux-browsers --install "$browser"
done
for browser in edge chrome; do
    profile="/tmp/${browser}-profile"
    mkdir -p "$profile"
    # Les services DBus ne sont volontairement pas présents dans cette VM de
    # test. Isoler le profil et /dev/shm évite que Chromium/Edge attende ces
    # services avant de rendre le document local ; le sandbox reste actif.
    timeout 45 winlux-browsers --launch "$browser" --headless --disable-gpu --disable-dev-shm-usage --no-first-run --no-default-browser-check --disable-background-networking --user-data-dir="$profile" --dump-dom file:///tmp/browser-test.html > "/tmp/$browser-dom.log" 2>&1 &
    runner=$!
    for attempt in $(seq 1 45); do
        grep -q '<title>WINLUX_BROWSER_OK</title>' "/tmp/$browser-dom.log" && break
        kill -0 "$runner" 2>/dev/null || break
        sleep 1
    done
    # Edge may keep background services alive after --dump-dom. The diagnostic
    # owns this isolated account and explicitly stops it after observing render.
    /bin/busybox pkill -TERM -u "$(id -u winlux-web)" 2>/dev/null || true
    wait "$runner" || true
    grep -q '<title>WINLUX_BROWSER_OK</title>' "/tmp/$browser-dom.log"
    echo "WINLUX_BROWSER_${browser}_RESULT=0"
done
timeout 45 winlux-browsers --launch firefox --headless --screenshot /home/winlux-web/browser-test.png file:///tmp/browser-test.html > /tmp/firefox-render.log 2>&1
test -s /home/winlux-web/browser-test.png
echo 'WINLUX_BROWSER_firefox_RESULT=0'
