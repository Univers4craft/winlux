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
for directory in dbus-1 xdg gtk-3.0 fonts; do
    if test -d "$components/etc/$directory"; then
        mkdir -p "$stage/etc/$directory"
        cp -a --remove-destination "$components/etc/$directory/." "$stage/etc/$directory/"
    fi
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
mkdir -p "$stage/usr/share/themes" "$stage/usr/share/xfce4/helpers"
cp -a "$project/integration/native/themes/." "$stage/usr/share/themes/"
cp -a "$project/integration/native/helpers/." "$stage/usr/share/xfce4/helpers/"
cp -a "$project/integration/native/applications/." "$stage/usr/share/applications/"
cp --remove-destination "$components/usr/libexec/xscreensaver/xscreensaver-gfx" "$stage/usr/libexec/xscreensaver/xscreensaver-gfx.real"
cp --remove-destination "$project/scripts/winlux-saver-gfx.py" "$stage/usr/libexec/xscreensaver/xscreensaver-gfx"
chmod 755 "$stage/usr/libexec/xscreensaver/xscreensaver-gfx" "$stage/usr/libexec/xscreensaver/xscreensaver-gfx.real"
for pair in 'winlux-native-session.sh:winlux-native-session' 'winlux-native-config.py:winlux-native-config' 'winlux-shell-app.sh:winlux-shell-app' 'winlux-settings.py:winlux-settings' 'winlux-desktop-action.sh:winlux-desktop-action' 'winlux-tray.py:winlux-tray' 'winlux-applications.py:winlux-applications' 'winlux-launch-exe.sh:winlux-launch-exe' 'winlux-start-menu.py:winlux-start-menu'; do
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
