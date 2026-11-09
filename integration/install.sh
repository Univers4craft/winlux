#!/bin/sh
# Installe Winlux et l'associe aux fichiers .exe (double-clic et binfmt).
#
#   sudo integration/install.sh            # installe dans /usr
#   sudo integration/install.sh /usr/local
#   DESTDIR=/tmp/racine integration/install.sh   # pour une image ou un paquet
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
prefix=${1:-/usr}
destdir=${DESTDIR:-}
bin="$destdir$prefix/bin"
share="$destdir$prefix/share"
etc="$destdir/etc"
version=${WINLUX_VERSION:-$(git -C "$root" describe --tags --always 2>/dev/null || printf 'development')}
mime_types="application/x-ms-dos-executable application/x-msdownload application/vnd.microsoft.portable-executable application/x-dosexec application/x-msi application/x-ms-shortcut"
web_types="x-scheme-handler/http x-scheme-handler/https text/html application/xhtml+xml"

test -x "$root/target/release/winlux" || {
    echo "target/release/winlux absent : lancez d'abord cargo build --release -p winlux" >&2
    exit 1
}

install -Dm755 "$root/target/release/winlux" "$bin/winlux"
install -Dm755 "$root/integration/winlux-open" "$bin/winlux-open"
install -Dm755 "$root/integration/winlux-browser" "$bin/winlux-browser"
install -Dm755 "$root/scripts/winlux-report.py" "$bin/winlux-report"
# Fenêtre UAC (requireAdministrator) et sa vérification de mot de passe.
install -Dm755 "$root/scripts/winlux-uac.py" "$bin/winlux-uac"
install -Dm644 "$root/scripts/winlux-lock.py" "$share/winlux/winlux-lock.py"
install -Dm755 "$root/scripts/install-browser.sh" "$share/winlux/install-browser.sh"
install -Dm755 "$root/scripts/prepare-browser-packages.py" "$share/winlux/winlux-browser-fetch"
install -Dm755 "$root/scripts/winlux-update.sh" "$bin/winlux-update"
install -Dm755 "$root/integration/uninstall.sh" "$share/winlux/uninstall.sh"
printf '%s\n' "$version" | install -Dm644 /dev/stdin "$share/winlux/version"
sed "s|/usr/bin/winlux-open|$prefix/bin/winlux-open|" "$root/integration/winlux.desktop" \
    | install -Dm644 /dev/stdin "$share/applications/winlux.desktop"
sed "s|/usr/bin/winlux-browser|$prefix/bin/winlux-browser|" "$root/integration/winlux-browser.desktop" \
    | install -Dm644 /dev/stdin "$share/applications/winlux-browser.desktop"
install -Dm644 "$root/integration/x-ms-dos-executable.xml" "$share/mime/packages/winlux.xml"
sed "s|/usr/bin/winlux|$prefix/bin/winlux|" "$root/integration/winlux-binfmt.conf" \
    | install -Dm644 /dev/stdin "$etc/binfmt.d/winlux.conf"
# Manettes lisibles par hid.dll (#271) : /dev/hidraw* des fabricants de jeu.
install -Dm644 "$root/integration/70-winlux-manettes.rules" "$etc/udev/rules.d/70-winlux-manettes.rules"

# Application par défaut des .exe pour tous les utilisateurs; uninstall.sh
# retire exactement ces lignes.
mimeapps="$etc/xdg/mimeapps.list"
mkdir -p "$etc/xdg"
touch "$mimeapps"
if ! grep -q '^\[Default Applications\]' "$mimeapps"; then
    printf '[Default Applications]\n' >>"$mimeapps"
fi
# Liens web : winlux-browser, qui ouvre le navigateur choisi par l'utilisateur.
for type in $web_types; do
    if grep -q "^$type=.*winlux-browser\.desktop" "$mimeapps"; then
        continue
    elif grep -q "^$type=" "$mimeapps"; then
        sed -i "s|^$type=|$type=winlux-browser.desktop;|" "$mimeapps"
    else
        sed -i "/^\[Default Applications\]/a $type=winlux-browser.desktop;" "$mimeapps"
    fi
done
for type in $mime_types; do
    if grep -q "^$type=.*winlux\.desktop" "$mimeapps"; then
        continue
    elif grep -q "^$type=" "$mimeapps"; then
        # Une autre application était choisie : Winlux passe en premier.
        sed -i "s|^$type=|$type=winlux.desktop;|" "$mimeapps"
    else
        sed -i "/^\[Default Applications\]/a $type=winlux.desktop;" "$mimeapps"
    fi
done

# Lancement direct des .exe (« ./programme.exe ») par binfmt_misc. systemd
# relit /etc/binfmt.d ; sans systemd (Devuan, Void, Alpine, Gentoo OpenRC),
# rien n'était enregistré avant le redémarrage (#27) : binfmt-support
# (update-binfmts, aussi au démarrage) s'il est là, sinon écriture directe
# dans /proc/sys/fs/binfmt_misc/register. WINLUX_BINFMT force la méthode
# (tests) : auto, systemd, update-binfmts ou proc.
binfmt=/proc/sys/fs/binfmt_misc
binfmt_line() { grep -v '^#' "$etc/binfmt.d/winlux.conf" | grep -m 1 .; }
binfmt_mount() {
    test -e "$binfmt/register" && return 0
    modprobe binfmt_misc 2>/dev/null || true
    test -d "$binfmt" && mount -t binfmt_misc binfmt_misc "$binfmt" 2>/dev/null || true
    test -e "$binfmt/register"
}
binfmt_proc() {
    binfmt_mount || return 1
    # Une ancienne entrée (autre préfixe) est remplacée par la nouvelle.
    if [ -e "$binfmt/winlux" ]; then echo -1 >"$binfmt/winlux" || return 1; fi
    binfmt_line >"$binfmt/register"
}
register_binfmt() {
    method=${WINLUX_BINFMT:-auto}
    if [ "$method" = auto ]; then
        if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then method=systemd
        elif command -v update-binfmts >/dev/null 2>&1; then method=update-binfmts
        else method=proc; fi
    fi
    case "$method" in
        systemd)
            binfmt_mount || true
            systemctl restart systemd-binfmt.service 2>/dev/null || true
            # systemd-binfmt absent ou en échec : enregistrement direct.
            [ -e "$binfmt/winlux" ] || binfmt_proc || true ;;
        update-binfmts)
            binfmt_mount || true
            # Gardé par binfmt-support, réenregistré à chaque démarrage.
            if [ -e "$binfmt/winlux" ]; then echo -1 >"$binfmt/winlux" || true; fi
            update-binfmts --package winlux --remove winlux "$bin/winlux" >/dev/null 2>&1 || true
            update-binfmts --package winlux --install winlux "$bin/winlux" --magic MZ --fix-binary yes \
                || binfmt_proc || true ;;
        proc)
            binfmt_proc || true
            echo "Note : sans systemd ni binfmt-support, /etc/binfmt.d/winlux.conf n'est relu au démarrage que si le système d'init le gère (service binfmt d'OpenRC par exemple)." >&2 ;;
        *) echo "WINLUX_BINFMT inconnu : $method" >&2; exit 2 ;;
    esac
    if [ -e "$binfmt/winlux" ]; then
        echo "Lancement direct des .exe actif ($binfmt/winlux)"
    else
        echo "Attention : lancement direct des .exe non enregistré ; « winlux programme.exe » et le double-clic restent disponibles." >&2
    fi
}

if [ -z "$destdir" ]; then
    update-mime-database "$share/mime" 2>/dev/null || true
    update-desktop-database "$share/applications" 2>/dev/null || true
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
    register_binfmt
fi
echo "Winlux $version installé dans $prefix/bin (double-clic : $prefix/bin/winlux-open)"
