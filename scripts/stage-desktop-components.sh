#!/bin/sh
# Native window management, panel, maintained editor and real GL pipes saver.
set -eu
stage=${1:?stage required}
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
components=${WINLUX_DESKTOP_COMPONENTS:-"$project/build/desktop-components/root"}
for program in openbox xfdesktop xfce4-panel xfce4-popup-whiskermenu mousepad Thunar xscreensaver xscreensaver-command wmctrl xdotool dbus-daemon python3; do
    test -x "$components/usr/bin/$program" || { echo "Composant natif absent : $program ; préparez les paquets du bureau." >&2; exit 1; }
done
# Never follow a pre-existing BusyBox applet link while copying a real tool.
cp -a --remove-destination "$components/usr/." "$stage/usr/"
# Some Ubuntu packages still ship outside /usr (notably cryptsetup and udev).
# Retain those dependencies as well as the merged-/usr package contents.
for directory in lib lib64 bin sbin; do
    if test -d "$components/$directory" && ! test -L "$components/$directory"; then
        mkdir -p "$stage/$directory"
        cp -a --remove-destination "$components/$directory/." "$stage/$directory/"
    fi
done
for directory in dbus-1 xdg gtk-3.0 fonts NetworkManager bluetooth udisks2 polkit-1; do
    if test -d "$components/etc/$directory"; then
        mkdir -p "$stage/etc/$directory"
        cp -a --remove-destination "$components/etc/$directory/." "$stage/etc/$directory/"
    fi
done
# Firmwares (Wi-Fi, son SOF, cartes graphiques, Bluetooth) : Ubuntu 26.04 les
# installe dans /usr/lib/firmware, mais le noyau ne les cherche que dans
# /lib/firmware, et /lib est ici un vrai dossier (pas le lien vers usr/lib
# d'Ubuntu). Sans ce lien, aucun firmware n'était chargé sur un vrai PC :
# portable récent sans haut-parleurs ni micro (« Sortie factice »), Wi-Fi Intel
# muet.
if test -d "$stage/usr/lib/firmware" && ! test -L "$stage/lib"; then
    if test -d "$stage/lib/firmware" && ! test -L "$stage/lib/firmware"; then
        cp -a --remove-destination "$stage/lib/firmware/." "$stage/usr/lib/firmware/"
        rm -rf "$stage/lib/firmware"
    fi
    mkdir -p "$stage/lib"
    ln -sfn ../usr/lib/firmware "$stage/lib/firmware"
fi
mkdir -p "$stage/usr/lib/NetworkManager/conf.d"
cp "$project/integration/native/network/90-winlux.conf" "$stage/usr/lib/NetworkManager/conf.d/90-winlux.conf"
# Sans logind, polkit ne voit aucune session « active » : nmcli lancé par un
# compte du bureau recevait « non autorisé ». Les membres de netdev (tous les
# comptes créés par Winlux) pilotent le réseau, comme sous Windows.
# /dev/rfkill ouvert au groupe netdev (#604) : sans logind, blueman-applet ne
# pouvait pas le lire. Dans /usr pour arriver aussi par la mise à jour.
mkdir -p "$stage/usr/lib/udev/rules.d"
install -m 644 "$project/integration/native/udev/60-winlux-rfkill.rules" "$stage/usr/lib/udev/rules.d/60-winlux-rfkill.rules"
mkdir -p "$stage/etc/polkit-1/rules.d"
install -m 644 "$project/integration/native/network/50-winlux-network.rules" "$stage/etc/polkit-1/rules.d/50-winlux-network.rules"
# Empreinte digitale (#659) : fprintd n'obéit qu'à root, donc au relais
# winlux-root qui a vérifié le compte ; aucun droit sur le lecteur lui-même.
# Dans /usr pour arriver aussi par la mise à jour.
mkdir -p "$stage/usr/share/polkit-1/rules.d"
install -m 644 "$project/integration/native/fprint/49-winlux-fprint.rules" "$stage/usr/share/polkit-1/rules.d/49-winlux-fprint.rules"
# Clé publique des mises à jour signées (#31) : présente, elle rend la
# signature Ed25519 du manifeste obligatoire.
if test -f "$project/integration/native/keys/update-signing.pub"; then
    mkdir -p "$stage/usr/share/winlux"
    install -m 644 "$project/integration/native/keys/update-signing.pub" "$stage/usr/share/winlux/update-signing.pub"
fi
for name in winlux-ab winlux-system-tools winlux-assistant winlux-notifications winlux-personalize winlux-session-startup winlux-popup winlux-widgets winlux-update-apt winlux-update-progress winlux-signature winlux-power winlux-trust-shortcuts winlux-welcome winlux-report winlux-lock winlux-permission winlux-uac winlux-signin winlux-root winlux-wifi winlux-vpn winlux-hotspot winlux-brightness winlux-volume winlux-certificates winlux-force-close winlux-close-watch winlux-maximized-watch winlux-phone winlux-parental winlux-webview winlux-screensaver-inhibit; do
    cp --remove-destination "$project/scripts/$name.py" "$stage/usr/bin/$name"
    chmod 755 "$stage/usr/bin/$name"
done
# Both legacy /lib and merged-/usr search locations exist in this small OS.
# Keep libc and its loader coherent, not shadowed by the earlier host copies.
mkdir -p "$stage/lib/x86_64-linux-gnu" "$stage/lib64"
for library in "$components"/usr/lib/x86_64-linux-gnu/*.so*; do
    test -e "$library" || continue
    cp -a --remove-destination "$library" "$stage/lib/x86_64-linux-gnu/"
done
cp -L --remove-destination "$components/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2" "$stage/lib64/ld-linux-x86-64.so.2"
mkdir -p "$stage/usr/share/winlux/native"
cp -a "$project/integration/native/." "$stage/usr/share/winlux/native/"
mkdir -p "$stage/usr/share/dbus-1/services"
cp "$project/integration/native/dbus/org.freedesktop.Notifications.service" "$stage/usr/share/dbus-1/services/"
# Vidéo et jeux sans économiseur : xscreensaver-systemd s'arrête sans logind,
# Winlux répond lui-même à org.freedesktop.ScreenSaver.Inhibit.
cp "$project/integration/native/dbus/org.freedesktop.ScreenSaver.service" "$stage/usr/share/dbus-1/services/"
# « Afficher dans le dossier » des navigateurs et des applications passe par
# org.freedesktop.FileManager1 : c'est l'Explorateur Winlux, plus Thunar
# (org.xfce.Thunar.FileManager1.service retiré, un seul fournisseur).
grep -l '^Name=org.freedesktop.FileManager1$' "$stage"/usr/share/dbus-1/services/*.service 2>/dev/null | while IFS= read -r other; do
    rm -f "$other"
done
cp "$project/integration/native/dbus/org.freedesktop.FileManager1.service" "$stage/usr/share/dbus-1/services/"
mkdir -p "$stage/usr/share/themes" "$stage/usr/share/xfce4/helpers"
cp -a "$project/integration/native/themes/." "$stage/usr/share/themes/"
# Identité Winlux : icônes Prisme claires (Winlux) et sombres (Winlux-Sombre,
# hérite de Winlux), complétées par Adwaita, et sons système.
mkdir -p "$stage/usr/share/icons" "$stage/usr/share/winlux/sounds"
for icon_theme in Winlux Winlux-Sombre; do
    rm -rf "$stage/usr/share/icons/$icon_theme"
    cp -a "$project/integration/native/icons/$icon_theme" "$stage/usr/share/icons/$icon_theme"
done
cp "$project"/assets/sounds/*.wav "$stage/usr/share/winlux/sounds/"
cp -a "$project/integration/native/helpers/." "$stage/usr/share/xfce4/helpers/"
cp -a "$project/integration/native/applications/." "$stage/usr/share/applications/"
# « Gestionnaire de fichiers » (exo) n'ouvrait que l'Explorateur Winlux :
# doublon retiré du menu (#721). exo-open --launch FileManager reste réglé
# sur l'Explorateur par helpers.rc.
rm -f "$stage/usr/share/applications/xfce4-file-manager.desktop"
# Archives (zip, 7z, rar, tar…) : double-clic = Explorer, qui les ouvre comme
# un dossier avec « Extraire tout », comme sous Windows 11. Un choix de
# l'utilisateur (~/.config/mimeapps.list) reste prioritaire.
mimeapps="$stage/etc/xdg/mimeapps.list"
mkdir -p "$(dirname -- "$mimeapps")"
test -f "$mimeapps" || printf '[Default Applications]\n' >"$mimeapps"
grep -q '^\[Default Applications\]' "$mimeapps" || printf '[Default Applications]\n' >>"$mimeapps"
sed -n 's/^MimeType=inode\/directory;//p' "$project/integration/native/applications/winlux-explorer.desktop" | tr ';' '\n' |
while IFS= read -r type; do
    test -n "$type" || continue
    grep -q "^$type=" "$mimeapps" && continue
    sed -i "/^\[Default Applications\]/a $type=winlux-explorer.desktop;" "$mimeapps"
done
# Dossiers : toujours l'Explorateur (xdg-open, GIO, liens des navigateurs et
# des .exe), y compris dans la liste propre à XFCE, qui passe avant.
for list in "$mimeapps" "$stage/etc/xdg/xfce-mimeapps.list" "$stage/usr/share/applications/xfce-mimeapps.list"; do
    test -f "$list" || continue
    sed -i '/^inode\/directory=/d' "$list"
    grep -q '^\[Default Applications\]' "$list" || printf '[Default Applications]\n' >>"$list"
    sed -i '/^\[Default Applications\]/a inode/directory=winlux-explorer.desktop;' "$list"
done
# Icônes des .exe et des raccourcis .lnk (Bureau, Explorateur, Thunar) via tumbler.
mkdir -p "$stage/usr/share/thumbnailers"
cp "$project/integration/native/thumbnailers/winlux-windows.thumbnailer" "$stage/usr/share/thumbnailers/"
cp --remove-destination "$components/usr/libexec/xscreensaver/xscreensaver-gfx" "$stage/usr/libexec/xscreensaver/xscreensaver-gfx.real"
cp --remove-destination "$project/scripts/winlux-saver-gfx.py" "$stage/usr/libexec/xscreensaver/xscreensaver-gfx"
chmod 755 "$stage/usr/libexec/xscreensaver/xscreensaver-gfx" "$stage/usr/libexec/xscreensaver/xscreensaver-gfx.real"
# xscreensaver-systemd plante sans systemd ni logind (#634) : remplacé par un
# assistant qui attend la fin de XScreenSaver, sans erreur ni nom D-Bus.
cp --remove-destination "$project/scripts/winlux-xscreensaver-systemd.sh" "$stage/usr/libexec/xscreensaver/xscreensaver-systemd"
chmod 755 "$stage/usr/libexec/xscreensaver/xscreensaver-systemd"
cp --remove-destination "$project/scripts/winlux-systemctl.sh" "$stage/usr/bin/systemctl"
chmod 755 "$stage/usr/bin/systemctl"
cp --remove-destination "$project/scripts/winlux-apt-service-hook.sh" "$stage/usr/share/winlux/apt-service-hook"
chmod 755 "$stage/usr/share/winlux/apt-service-hook"
cp --remove-destination "$project/scripts/winlux-boot-menu.sh" "$stage/usr/share/winlux/boot-menu"
chmod 755 "$stage/usr/share/winlux/boot-menu"
for pair in 'winlux-native-session.sh:winlux-native-session' 'winlux-user-session.sh:winlux-user-session' 'winlux-system-services.sh:winlux-system-services' 'winlux-network-watch.sh:winlux-network-watch' 'winlux-network-perms.sh:winlux-network-perms' 'winlux-native-config.py:winlux-native-config' 'winlux-shell-app.sh:winlux-shell-app' 'winlux-settings.py:winlux-settings' 'winlux-settings-pages.py:winlux-settings-pages' 'winlux-update-page.py:winlux-update-page' 'winlux-reset-page.py:winlux-reset-page' 'winlux-hardware-page.py:winlux-hardware-page' 'winlux-osk.py:winlux-osk' 'winlux-mouse.py:winlux-mouse' 'winlux-input-watch.sh:winlux-input-watch' 'winlux-desktop-action.sh:winlux-desktop-action' 'winlux-desktop-paste.py:winlux-desktop-paste' 'winlux-tray.py:winlux-tray' 'winlux-applications.py:winlux-applications' 'winlux-launch-exe.sh:winlux-launch-exe' 'winlux-start-menu.py:winlux-start-menu' 'winlux-default-apps.py:winlux-default-apps' 'winlux-installed-apps.py:winlux-installed-apps' 'winlux-notify-icon.py:winlux-notify-icon' 'winlux-open-with.py:winlux-open-with' 'winlux-browser-choice.py:winlux-browser-choice' 'winlux-clock.sh:winlux-clock' 'winlux-power-modes.py:winlux-power-modes' 'winlux-ambiance.py:winlux-ambiance' 'winlux-emoji.py:winlux-emoji' 'winlux-clipboard.py:winlux-clipboard' 'winlux-snap.py:winlux-snap' 'winlux-super-key.py:winlux-super-key' 'winlux-run.py:winlux-run' 'winlux-taskview.py:winlux-taskview' 'winlux-alarms.py:winlux-alarms' 'winlux-colorpicker.py:winlux-colorpicker' 'winlux-rename.py:winlux-rename' 'winlux-ocr.py:winlux-ocr' 'winlux-speech.py:winlux-speech' 'winlux-first-boot-notice.sh:winlux-first-boot-notice' 'winlux-accounts.py:winlux-accounts' 'winlux-setup-wizard.py:winlux-setup-wizard'; do
    source=${pair%:*}; destination=${pair#*:}
    cp --remove-destination "$project/scripts/$source" "$stage/usr/bin/$destination"
    chmod 755 "$stage/usr/bin/$destination"
done
"$components/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2" --library-path "$components/usr/lib/x86_64-linux-gnu" \
    "$components/usr/bin/glib-compile-schemas" "$stage/usr/share/glib-2.0/schemas"
for database in mime desktop; do
    directory=mime; test "$database" != desktop || directory=applications
    "$components/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2" --library-path "$components/usr/lib/x86_64-linux-gnu" \
        "$components/usr/bin/update-$database-database" "$stage/usr/share/$directory"
done
if test -d "$components/usr/share/i18n/locales" && command -v localedef >/dev/null 2>&1; then
    mkdir -p "$stage/usr/lib/locale/fr_FR.utf8"
    I18NPATH="$components/usr/share/i18n" localedef --no-archive -i fr_FR -f UTF-8 "$stage/usr/lib/locale/fr_FR.utf8" || true
fi
# Only compiled locales are used at runtime. Locale source/charmap filenames
# include commas that the update format deliberately refuses.
rm -rf "$stage/usr/share/i18n"
# Preserve this CA's bytes under an ASCII name accepted by the update manifest.
certificate="$stage/usr/share/ca-certificates/mozilla/NetLock_Arany_=Class_Gold=_Főtanúsítvány.crt"
if test -f "$certificate"; then
    mv "$certificate" "$stage/usr/share/ca-certificates/mozilla/NetLock_Arany_Class_Gold_CA.crt"
fi
# Liste des autorités actives d'update-ca-certificates (créée d'ordinaire par
# le script d'installation du paquet, qui ne tourne pas ici) : sans elle, une
# mise à jour de ca-certificates ou l'ajout d'une autorité d'entreprise
# reconstruirait un magasin vide.
if test -d "$stage/usr/share/ca-certificates/mozilla" && ! test -s "$stage/etc/ca-certificates.conf"; then
    (cd "$stage/usr/share/ca-certificates" && find mozilla -name '*.crt' | LC_ALL=C sort) >"$stage/etc/ca-certificates.conf"
fi
# Dossier /etc/ssl/certs avec un fichier par autorité et ses liens par
# empreinte (openssl rehash) : le Wi-Fi d'entreprise (« Certificats du
# système » : wpa_supplicant lit ce dossier, pas le fichier groupé) et les
# programmes qui utilisent un CApath ne trouvaient aucune autorité.
if test -d "$stage/usr/share/ca-certificates/mozilla"; then
    mkdir -p "$stage/etc/ssl/certs"
    for certificate in "$stage"/usr/share/ca-certificates/mozilla/*.crt; do
        name=${certificate##*/}
        ln -sfn "../../../usr/share/ca-certificates/mozilla/$name" "$stage/etc/ssl/certs/${name%.crt}.pem"
    done
    if command -v openssl >/dev/null 2>&1; then
        openssl rehash "$stage/etc/ssl/certs" 2>/dev/null || c_rehash "$stage/etc/ssl/certs" >/dev/null 2>&1 || true
    fi
fi
