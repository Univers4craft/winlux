#!/bin/sh
# WebView2 maison (#236) : compile testc/msvc/webview2_msvc.c avec clang
# (cible MSVC) si besoin, puis le lance sous Winlux avec le moteur web
# scripts/winlux-webview.py (WebKitGTK) sous Xvfb. Le programme charge
# WebView2Loader.dll, crée environnement et contrôleur, échange des
# messages avec la page, exécute un script et ouvre un hôte virtuel.
# Sauté (code 0) sans runtime, clang, xvfb-run ou WebKitGTK (gir1.2-webkit2-4.1).
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/webview2_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/webview2_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }
command -v xvfb-run >/dev/null 2>&1 || { echo "xvfb-run absent : test sauté"; exit 0; }
python=
for p in python3 python3.12 python3.11 python3.13; do
    if command -v "$p" >/dev/null 2>&1 && "$p" -c "import gi; gi.require_version('WebKit2', '4.1'); from gi.repository import WebKit2" >/dev/null 2>&1; then
        python=$p
        break
    fi
done
test -n "$python" || { echo "WebKitGTK (gir1.2-webkit2-4.1) absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/home" "$work/run" "$work/site"
chmod 700 "$work/run"
cat >"$work/site/index.html" <<'HTML'
<!doctype html><html><body><script>window.chrome.webview.postMessage("hote-virtuel");</script></body></html>
HTML
set +e
HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_GUI=headless \
XDG_RUNTIME_DIR="$work/run" WINLUX_WEBVIEW_HELPER="$python $root/scripts/winlux-webview.py" \
WINLUX_WEBVIEW_NO_INSTALL=1 timeout 90 xvfb-run -a "$runtime" "$exe" "$work/site" >"$work/out.txt" 2>&1
rc=$?
set -e
if test "$rc" -ne 0; then
    cat "$work/out.txt"
    echo "webview2 : échec, code $rc (voir testc/msvc/webview2_msvc.c)" >&2
    exit 1
fi
echo "webview2 : environnement, contrôleur, messages, ExecuteScript et hôte virtuel sur WebKitGTK"
