#!/bin/sh
# Explicit diagnostic boot in a disposable guest; never used by normal startup.
set -eu
mkdir -p /home/winlux-web
printf '<!doctype html><title>WINLUX_BROWSER_OK</title><h1>Winlux</h1>\n' > /tmp/browser-test.html
chmod 644 /tmp/browser-test.html
for browser in edge chrome firefox; do
    winlux-browsers --install "$browser"
done
for browser in edge chrome; do
    timeout 45 winlux-browsers --launch "$browser" --headless --disable-gpu --no-first-run --dump-dom file:///tmp/browser-test.html > "/tmp/$browser-dom.log" 2>&1
    grep -q '<title>WINLUX_BROWSER_OK</title>' "/tmp/$browser-dom.log"
    echo "WINLUX_BROWSER_${browser}_RESULT=0"
done
timeout 45 winlux-browsers --launch firefox --headless --screenshot /home/winlux-web/browser-test.png file:///tmp/browser-test.html > /tmp/firefox-render.log 2>&1
test -s /home/winlux-web/browser-test.png
echo 'WINLUX_BROWSER_firefox_RESULT=0'
