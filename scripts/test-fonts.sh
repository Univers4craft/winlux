#!/bin/sh
# Teste le service des polices (winlux-fonts) avec fontconfig simulé.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-fonts-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/bin" "$work/root" "$work/home"
export WINLUX_CONTROL_ROOT="$work/root"
export HOME="$work/home" XDG_DATA_HOME="$work/home/.local/share"
export LOG="$work/commands" WINLUX_LANG=fr
export PATH="$work/bin:$PATH"
run="$work/root/run/winlux"
fail() { echo "ÉCHEC : $*" >&2; exit 1; }

cat > "$work/bin/id" <<'MOCK'
#!/bin/sh
echo 0
MOCK
cat > "$work/bin/fc-cache" <<'MOCK'
#!/bin/sh
printf 'fc-cache %s\n' "$*" >> "$LOG"
MOCK
cat > "$work/bin/fc-list" <<MOCK
#!/bin/sh
echo 'DejaVu Sans|Book|/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'
for f in "$work/home/.local/share/fonts"/*; do
    test -f "\$f" && echo "Ma Police|Regular|\$f"
done
for f in "$work/home/Windows/Fonts"/*; do
    test -f "\$f" && echo "Police Windows|Regular|\$f"
done
echo 'Bitmap|Regular|/usr/share/fonts/X11/misc/6x13.pcf.gz'
MOCK
chmod 755 "$work/bin/"*
fonts() { sh "$project/scripts/winlux-fonts.sh" "$@"; }

# --- Polices ---
printf '\000\001\000\000police' > "$work/police.ttf"
printf 'pas une police' > "$work/faux.ttf"
fonts install "$work/police.ttf"
test -f "$XDG_DATA_HOME/fonts/police.ttf" || fail 'police copiée'
grep -qx 'state=done' "$run/fonts-action.txt" || fail 'installation police'
grep -q "fc-cache -f $XDG_DATA_HOME/fonts" "$LOG" || fail 'cache fontconfig'
grep -qx "font=Ma Police|Regular|user|$XDG_DATA_HOME/fonts/police.ttf" "$run/fonts.txt" || fail 'police utilisateur listée'
grep -qx 'font=DejaVu Sans|Book|system|/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf' "$run/fonts.txt" || fail 'police système listée'
if grep -q 'pcf' "$run/fonts.txt"; then fail 'police bitmap listée'; fi
if fonts install "$work/faux.ttf" 2>/dev/null; then fail 'fausse police acceptée'; fi
if fonts remove /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf 2>/dev/null; then fail 'police système supprimée'; fi
grep -q 'protégées' "$run/fonts-action.txt" || fail 'message police système'
WINLUX_LANG=de fonts remove /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf 2>/dev/null || true
grep -q 'geschützt' "$run/fonts-action.txt" || fail 'message en allemand'
WINLUX_LANG=es fonts remove /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf 2>/dev/null || true
grep -q 'protegidas' "$run/fonts-action.txt" || fail 'message en espagnol'
fonts remove "$XDG_DATA_HOME/fonts/police.ttf"
test ! -e "$XDG_DATA_HOME/fonts/police.ttf" || fail 'police non supprimée'
if grep -q 'Ma Police' "$run/fonts.txt"; then fail 'liste non rafraîchie'; fi
# Police installée par une application Windows dans C:\Windows\Fonts :
# listée pour Linux et désinstallable comme une police de l'utilisateur.
mkdir -p "$work/home/Windows/Fonts"
printf '\000\001\000\000police' > "$work/home/Windows/Fonts/appli.ttf"
fonts list
grep -qx "font=Police Windows|Regular|user|$work/home/Windows/Fonts/appli.ttf" "$run/fonts.txt" || fail 'police Windows listée'
fonts remove 'C:\Windows\Fonts\appli.ttf'
test ! -e "$work/home/Windows/Fonts/appli.ttf" || fail 'police Windows non supprimée'
grep -q 'fontconfig' "$project/integration/native/config/fontconfig/conf.d/60-winlux-windows-fonts.conf" || fail 'configuration fontconfig absente'
grep -q '<dir>~/Windows/Fonts</dir>' "$project/integration/native/config/fontconfig/conf.d/60-winlux-windows-fonts.conf" || fail 'C:\Windows\Fonts absent de fontconfig'
echo 'Polices : OK'
