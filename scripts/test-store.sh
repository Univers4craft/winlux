#!/bin/sh
# Teste les services du Store (winlux-store) et des polices (winlux-fonts)
# avec des outils apt/fontconfig simulés.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-store-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/bin" "$work/root" "$work/home"
export WINLUX_CONTROL_ROOT="$work/root"
export WINLUX_STORE_CATALOG="$project/integration/store-catalog.txt"
export HOME="$work/home" XDG_DATA_HOME="$work/home/.local/share"
export LOG="$work/commands"
export PATH="$work/bin:$PATH"
run="$work/root/run/winlux"
fail() { echo "ÉCHEC : $*" >&2; exit 1; }

cat > "$work/bin/id" <<'MOCK'
#!/bin/sh
echo 0
MOCK
cat > "$work/bin/dpkg-query" <<'MOCK'
#!/bin/sh
shift 3
for p in "$@"; do
    case "$p" in
        vlc) echo 'vlc|ii |3.0.20-1|12000' ;;
        git) echo 'git|ii |1:2.43.0-1|9000' ;;
    esac
done
MOCK
cat > "$work/bin/apt-cache" <<'MOCK'
#!/bin/sh
case "$1" in
    policy) shift; for p in "$@"; do
        echo "$p:"
        case "$p" in
            vlc) echo '  Installed: 3.0.20-1'; echo '  Candidate: 3.0.21-1' ;;
            git) echo '  Installed: 1:2.43.0-1'; echo '  Candidate: 1:2.43.0-1' ;;
            0ad) echo '  Installed: (none)'; echo '  Candidate: (none)' ;;
            *) echo '  Installed: (none)'; echo '  Candidate: 1.0-1' ;;
        esac
    done ;;
    show) shift 2; for p in "$@"; do printf 'Package: %s\nInstalled-Size: 2048\n\n' "$p"; done ;;
esac
MOCK
cat > "$work/bin/apt-get" <<'MOCK'
#!/bin/sh
printf 'apt-get %s\n' "$*" >> "$LOG"
test "${APT_FAIL:-0}" = 0 || exit 100
echo 'dlstatus:1:40.0:Téléchargement' >&3
echo 'pmstatus:vlc:80.0:Installation de vlc' >&3
MOCK
cat > "$work/bin/winlux" <<'MOCK'
#!/bin/sh
printf 'winlux %s\n' "$*" >> "$LOG"
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
echo 'Bitmap|Regular|/usr/share/fonts/X11/misc/6x13.pcf.gz'
MOCK
chmod 755 "$work/bin/"*
store() { sh "$project/scripts/winlux-store.sh" "$@"; }
fonts() { sh "$project/scripts/winlux-fonts.sh" "$@"; }

# --- Catalogue ---
store refresh
grep -qx 'apt=1' "$run/store.txt" || fail 'apt non détecté'
grep -q '^app=vlc|multimedia|VLC|vlc|u|3.0.20-1|3.0.21-1|12000|' "$run/store.txt" || fail 'mise à jour VLC'
grep -q '^app=git|developpement|Git|git|i|' "$run/store.txt" || fail 'Git installé'
grep -q '^app=gimp|multimedia|GIMP|gimp|a||1.0-1|2048|' "$run/store.txt" || fail 'GIMP disponible'
grep -q '^app=0ad|jeux|0 A.D.|0ad|x|' "$run/store.txt" || fail 'paquet absent des sources'
grep -qx 'updates=1' "$run/store.txt" || fail 'compte des mises à jour'
for c in navigateurs bureautique multimedia developpement jeux utilitaires; do
    grep -q "^app=[^|]*|$c|" "$run/store.txt" || fail "catégorie $c vide"
done

# --- Installation apt ---
store install gimp
grep -q 'apt-get -y -q -o APT::Status-Fd=3 -o Dpkg::Use-Pty=0 install --no-install-recommends -- gimp' "$LOG" || fail 'commande apt'
grep -qx 'state=done' "$run/store-progress.txt" || fail 'installation terminée'
grep -qx 'percent=100' "$run/store-progress.txt" || fail 'progression finale'
store remove vlc
grep -q 'remove -- vlc' "$LOG" || fail 'désinstallation'
store upgrade-all
grep -q 'install --only-upgrade -- vlc' "$LOG" || fail 'mise à jour groupée'
if store install 'evil;rm' 2>/dev/null; then fail 'paquet invalide accepté'; fi
grep -q 'Paquet inconnu' "$run/store-progress.txt" || fail 'message paquet inconnu'
if store install coreutils 2>/dev/null; then fail 'paquet hors catalogue accepté'; fi
APT_FAIL=1 store install gimp || true
grep -qx 'state=error' "$run/store-progress.txt" || fail 'échec apt signalé'
mkdir "$run/store.lock"
if store install gimp 2>/dev/null; then fail 'verrou ignoré'; fi
rmdir "$run/store.lock"

# --- Installateurs Windows ---
mkdir -p "$work/home/Téléchargements"
: > "$work/home/Téléchargements/setup.exe"
store install-file 'C:\Téléchargements\setup.exe'
grep -q "winlux $work/home/Téléchargements/setup.exe" "$LOG" || fail 'installateur .exe'
grep -qx 'state=done' "$run/store-progress.txt" || fail 'installateur .exe terminé'
: > "$work/home/app.msi"
if store install-file "$work/home/app.msi" 2>/dev/null; then fail 'msi sans msiexec'; fi
grep -q 'msiexec' "$run/store-progress.txt" || fail 'message msiexec'
mkdir -p "$work/home/windows/system32"
: > "$work/home/windows/system32/msiexec.exe"
store install-file "$work/home/app.msi"
grep -q "msiexec.exe /i $work/home/app.msi" "$LOG" || fail 'msiexec lancé'
mkdir -p "$XDG_DATA_HOME/applications"
printf '[Desktop Entry]\nName=Notepad++\nX-Winlux-Executable=/c/npp.exe\n' > "$XDG_DATA_HOME/applications/winlux-pe-1.desktop"
store refresh
grep -qx 'win=Notepad++|/c/npp.exe' "$run/store.txt" || fail 'programme Windows installé'

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
fonts remove "$XDG_DATA_HOME/fonts/police.ttf"
test ! -e "$XDG_DATA_HOME/fonts/police.ttf" || fail 'police non supprimée'
if grep -q 'Ma Police' "$run/fonts.txt"; then fail 'liste non rafraîchie'; fi
echo 'Store et polices : OK'
