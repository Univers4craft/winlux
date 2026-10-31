#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out=${1:-"$root/build/winlux-live-test.iso"}
case "$out" in
    /*) ;;
    *) out="$root/$out" ;;
esac
work="$root/build/winlux-live-test"
stage="$work/initramfs-root"
apt_base=${WINLUX_APT_BASE:-"$root/build/apt-base/root.tar"}
if test "${WINLUX_LEGACY_BASE:-0}" != 1; then
    test -s "$apt_base" || { echo "Base APT native absente : préparez-la avec scripts/prepare-apt-base.py (--kernel et --kernel-release)." >&2; exit 1; }
    native_codename=$(. /etc/os-release; printf '%s\n' "${UBUNTU_CODENAME:-${VERSION_CODENAME:-}}")
    test "$native_codename" = resolute && test "$(dpkg --print-architecture)" = amd64 || {
        echo 'La construction APT native nécessite un hôte Ubuntu Resolute amd64 (ou dérivé compatible).' >&2
        exit 1
    }
fi
iso_root="$work/iso-root"
kernel=${WINLUX_KERNEL:-/boot/vmlinuz}
kernel_release=${WINLUX_KERNEL_RELEASE:-$(uname -r)}
virtio_dma_module="/lib/modules/$kernel_release/kernel/drivers/virtio/virtio_dma_buf.ko.zst"
virtio_gpu_module="/lib/modules/$kernel_release/kernel/drivers/gpu/drm/virtio/virtio-gpu.ko.zst"
virtio_input_module="/lib/modules/$kernel_release/kernel/drivers/virtio/virtio_input.ko.zst"
serio_raw_module="/lib/modules/$kernel_release/kernel/drivers/input/serio/serio_raw.ko.zst"
psmouse_module="/lib/modules/$kernel_release/kernel/drivers/input/mouse/psmouse.ko.zst"
hid_module="/lib/modules/$kernel_release/kernel/drivers/hid/hid.ko.zst"
usbhid_module="/lib/modules/$kernel_release/kernel/drivers/hid/usbhid/usbhid.ko.zst"
hid_generic_module="/lib/modules/$kernel_release/kernel/drivers/hid/hid-generic.ko.zst"
fonts=${WINLUX_X11_FONTS:-/usr/share/fonts/X11/misc}
xkb=${WINLUX_XKB_DATA:-/usr/share/X11/xkb}
xorg=${WINLUX_XORG:-/usr/lib/xorg/Xorg}
xorg_modules=${WINLUX_XORG_MODULES:-/usr/lib/xorg/modules}
truetype_fonts=${WINLUX_TRUETYPE_FONTS:-/usr/share/fonts/truetype/dejavu}
xft_library=${WINLUX_XFT_LIBRARY:-$(ldconfig -p | awk '$1 == "libXft.so.2" { print $NF; exit }')}
cairo_library=${WINLUX_CAIRO_LIBRARY:-$(ldconfig -p | awk '$1 == "libcairo.so.2" { print $NF; exit }')}

for tool in cargo bash cpio depmod modprobe fakeroot find gzip grub-mkrescue grub-script-check ldd ldconfig sha256sum xorriso zstd mksquashfs; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "outil requis absent : $tool" >&2
        exit 1
    }
done
if ! test -r "$kernel"; then
    echo "noyau absent ou illisible : $kernel" >&2
    echo "indiquez un noyau lisible avec WINLUX_KERNEL=/chemin/vers/vmlinuz" >&2
    exit 1
fi
for path in /usr/bin/busybox /usr/bin/xrandr /usr/bin/curl /usr/bin/install /usr/bin/udevadm /usr/bin/Xvfb /usr/bin/xkbcomp "$xorg" "$xorg_modules" "$fonts" "$xkb" "$truetype_fonts" /usr/lib/udev/rules.d /etc/fonts /etc/ssl/certs/ca-certificates.crt; do
    test -e "$path" || { echo "fichier requis absent : $path" >&2; exit 1; }
done
test -r /usr/share/X11/xorg.conf.d/40-libinput.conf || { echo "configuration libinput Xorg absente" >&2; exit 1; }
test -r "$xft_library" || { echo "bibliothèque Xft absente : $xft_library" >&2; exit 1; }
test -r "$cairo_library" || { echo "bibliothèque Cairo absente : $cairo_library" >&2; exit 1; }
for module in "$virtio_dma_module" "$virtio_gpu_module" "$virtio_input_module" "$serio_raw_module" "$psmouse_module" "$hid_module" "$usbhid_module" "$hid_generic_module"; do
    test -r "$module" || { echo "module noyau requis absent : $module" >&2; exit 1; }
done

mkdir -p "$root/build"
rm -rf "$work"
mkdir -p \
    "$stage/bin" "$stage/boot" "$stage/dev" "$stage/etc/winlux" "$stage/lib" "$stage/lib64" "$stage/sbin" \
    "$stage/lib/modules/$kernel_release" \
    "$stage/opt/winlux/scripts" "$stage/opt/winlux/tests" "$stage/proc" \
    "$stage/root/.local/share/winlux" "$stage/run" "$stage/sys" "$stage/tmp" \
    "$stage/usr/bin" "$stage/usr/lib" "$stage/usr/lib/systemd" "$stage/usr/lib/udev" "$stage/usr/lib/xorg" "$stage/usr/share/fonts/X11" \
    "$stage/usr/share/applications" "$stage/usr/share/fonts/truetype" \
    "$stage/usr/share/icons" "$stage/usr/share/pixmaps" \
    "$stage/usr/share/X11" "$stage/etc/fonts"

cd "$root"
cargo build --release -p winlux
bash scripts/build-tests.sh

cp "$root/target/release/winlux" "$stage/usr/bin/winlux"
cp "$root/scripts/winlux-update.sh" "$stage/usr/bin/winlux-update"
cp "$root/scripts/winlux-update-progress.py" "$stage/usr/bin/winlux-update-progress"
chmod 755 "$stage/usr/bin/winlux-update-progress"
cp "$root/scripts/winlux-control.sh" "$stage/usr/bin/winlux-control"
cp "$root/scripts/winlux-journal.sh" "$stage/usr/bin/winlux-journal"
cp "$root/scripts/winlux-restore.sh" "$stage/usr/bin/winlux-restore"
cp "$root/scripts/winlux-network.sh" "$stage/usr/bin/winlux-network"
cp "$root/scripts/winlux-display.sh" "$stage/usr/bin/winlux-display"
chmod 755 "$stage/usr/bin/winlux-display"
mkdir -p "$stage/usr/share/winlux"
cp "$root/scripts/winlux-dhcp-hook.sh" "$stage/usr/share/winlux/udhcpc.script"
cp "$root/integration/winlux-browser" "$stage/usr/bin/winlux-browser"
cp "$root/scripts/winlux-browsers.sh" "$stage/usr/bin/winlux-browsers"
chmod 755 "$stage/usr/bin/winlux-browsers"
cp "$root/integration/winlux-open" "$stage/usr/bin/winlux-open"
chmod 755 "$stage/usr/bin/winlux-browser" "$stage/usr/bin/winlux-open" "$stage/usr/bin/winlux-control" "$stage/usr/bin/winlux-journal" "$stage/usr/bin/winlux-restore" "$stage/usr/bin/winlux-network" "$stage/usr/share/winlux/udhcpc.script"

# Installateur et outils associés
cp "$root/scripts/winlux-installer.py" "$stage/usr/bin/winlux-installer"
cp "$root/scripts/winlux-hw-detect.sh" "$stage/usr/bin/winlux-hw-detect"
cp "$root/scripts/winlux-first-boot.sh" "$stage/usr/bin/winlux-first-boot"
cp "$root/scripts/install.sh" "$stage/opt/winlux/scripts/install.sh"
cp "$root/scripts/install-safety.sh" "$stage/opt/winlux/scripts/install-safety.sh"
chmod 755 "$stage/usr/bin/winlux-installer" "$stage/usr/bin/winlux-hw-detect" "$stage/usr/bin/winlux-first-boot" "$stage/opt/winlux/scripts/install.sh"
cp "$root/integration/winlux-installer.desktop" "$stage/usr/share/applications/winlux-installer.desktop"
mkdir -p "$stage/root/Bureau"
cp "$root/integration/winlux-installer.desktop" "$stage/root/Bureau/winlux-installer.desktop"
mkdir -p "$stage/usr/share/winlux"
printf '%s\n' "${WINLUX_BUILD_VERSION:-$(cat "$root/VERSION")}" >"$stage/usr/share/winlux/version"
sh "$root/scripts/build-info.sh" "$(cat "$stage/usr/share/winlux/version")" system >"$stage/usr/share/winlux/BUILD-INFO"
printf 'system\n' >"$stage/usr/share/winlux/update-scope"
mkdir -p "$stage/usr/share/winlux/avatars"
cp "$root"/assets/avatars/avatar-*.svg "$stage/usr/share/winlux/avatars/"

# Fond d'écran : conversion PNG → BMP 24-bit non compressé pour le shell.
wallpaper_src="$root/assets/wallpapers/Fond ecran.png"
if test -f "$wallpaper_src"; then
    if command -v convert >/dev/null 2>&1; then
        convert "$wallpaper_src" -filter Lanczos -resize '1920x1080^' -gravity center -extent 1920x1080 -type TrueColor BMP3:"$stage/usr/share/winlux/wallpaper.bmp"
    elif command -v ffmpeg >/dev/null 2>&1; then
        ffmpeg -hide_banner -loglevel error -i "$wallpaper_src" -vf 'scale=1920:1080:force_original_aspect_ratio=increase:flags=lanczos,crop=1920:1080,setsar=1' -frames:v 1 -pix_fmt bgr24 "$stage/usr/share/winlux/wallpaper.bmp"
    else
        echo 'fond PNG : ImageMagick (convert) ou ffmpeg requis' >&2
        exit 1
    fi
fi

cp "$root/scripts/test-runtime.sh" "$stage/opt/winlux/scripts/test-runtime.sh"
cp "$root/scripts/test-terminal-guest.sh" "$stage/opt/winlux/scripts/test-terminal-guest.sh"
cp "$root/scripts/test-native-session-guest.py" "$stage/opt/winlux/scripts/test-native-session-guest.py"
cp "$root/scripts/test-apt-native-guest.py" "$stage/opt/winlux/scripts/test-apt-native-guest.py"
cp "$root/scripts/test-browsers-guest.sh" "$stage/opt/winlux/scripts/test-browsers-guest.sh"
cp "$root/scripts/test-runtime-parallel.sh" "$stage/opt/winlux/scripts/test-runtime-parallel.sh"
chmod 755 \
    "$stage/usr/bin/winlux" "$stage/usr/bin/winlux-update" \
    "$stage/opt/winlux/scripts/test-runtime.sh" \
    "$stage/opt/winlux/scripts/test-runtime-parallel.sh"
for name in hello exitcode p1_suite dll_import delay_import findfiles crt_suite gui_suite gui_demo explorer_regression; do
    cp "$root/target/x86_64-pc-windows-msvc/release/$name.exe" "$stage/opt/winlux/tests/$name.exe"
done
cp "$root/target/x86_64-pc-windows-msvc/release/testdll.dll" "$stage/opt/winlux/tests/testdll.dll"

# Dossier personnel de la session live (C: dans Winlux) : dossiers usuels,
# un mot de bienvenue et un .exe d'exemple à ouvrir depuis l'Explorer.
for folder in Bureau Documents Images Musique "Téléchargements" "Vidéos"; do
    mkdir -p "$stage/root/$folder"
done
cat >"$stage/root/Documents/Bienvenue dans Winlux.txt" <<'WELCOME'
Bienvenue dans Winlux !

- Explorer (icône dossier de la barre des tâches) parcourt vos fichiers :
  C: est votre dossier personnel Linux, Z: la racine du système.
- Double-cliquez sur un .exe pour le lancer directement dans Winlux.
- Onglets : Ctrl+T, double volet : F3, recherche : Ctrl+F.
WELCOME
cp "$root/target/x86_64-pc-windows-msvc/release/hello.exe" "$stage/root/Téléchargements/Bonjour.exe"

copy_shared_libraries() {
    binary=$1
    ldd "$binary" | awk '$2 == "=>" && $3 ~ /^\// { print $3 } $1 ~ /^\// { print $1 }' | sort -u |
    while IFS= read -r lib; do
        test -n "$lib" || continue
        destination="$stage$lib"
        mkdir -p "$(dirname -- "$destination")"
        cp -L "$lib" "$destination"
    done
}

cp /usr/bin/busybox "$stage/bin/busybox"
chmod 755 "$stage/bin/busybox"
ln -s busybox "$stage/bin/sh"
ln -s busybox "$stage/bin/true"
ln -s ../../bin/busybox "$stage/usr/bin/dirname"
ln -s ../../bin/busybox "$stage/usr/bin/mktemp"
ln -s ../../bin/busybox "$stage/usr/bin/timeout"
for applet in awk cat chmod chown cmp cp cut date env find grep head id ip ipcalc kill killall ln ls mkdir modprobe mount mountpoint mv od printf ps pwd readlink reboot poweroff rm rmdir sed sha256sum sleep sort stat sync tar tee touch tr truncate umount uname wc xargs; do
    ln -s ../../bin/busybox "$stage/usr/bin/$applet"
done
ln -s ../bin/busybox "$stage/sbin/modprobe"
cp /bin/bash "$stage/bin/bash"
cp /usr/bin/curl "$stage/usr/bin/curl"
cp /usr/bin/install "$stage/usr/bin/install"
cp /usr/bin/udevadm "$stage/usr/bin/udevadm"
ln -s ../../bin/udevadm "$stage/usr/lib/systemd/systemd-udevd"
cp -a /usr/lib/udev/rules.d "$stage/usr/lib/udev/rules.d"
cp /usr/bin/Xvfb "$stage/usr/bin/Xvfb"
cp /usr/bin/xkbcomp "$stage/usr/bin/xkbcomp"
cp /usr/bin/xrandr "$stage/usr/bin/xrandr"
cp "$xorg" "$stage/usr/bin/Xorg"
zstd -q -d -c "$virtio_dma_module" >"$stage/lib/modules/$kernel_release/virtio_dma_buf.ko"
zstd -q -d -c "$virtio_gpu_module" >"$stage/lib/modules/$kernel_release/virtio-gpu.ko"
zstd -q -d -c "$virtio_input_module" >"$stage/lib/modules/$kernel_release/virtio-input.ko"
zstd -q -d -c "$serio_raw_module" >"$stage/lib/modules/$kernel_release/serio-raw.ko"
zstd -q -d -c "$psmouse_module" >"$stage/lib/modules/$kernel_release/psmouse.ko"
zstd -q -d -c "$hid_module" >"$stage/lib/modules/$kernel_release/hid.ko"
zstd -q -d -c "$usbhid_module" >"$stage/lib/modules/$kernel_release/usbhid.ko"
zstd -q -d -c "$hid_generic_module" >"$stage/lib/modules/$kernel_release/hid-generic.ko"
chmod 755 "$stage/usr/bin/Xvfb" "$stage/usr/bin/xkbcomp" "$stage/usr/bin/xrandr" "$stage/usr/bin/Xorg"
copy_shared_libraries "$root/target/release/winlux"
sh "$root/scripts/copy-audio-libraries.sh" "$stage"
copy_shared_libraries /usr/bin/Xvfb
copy_shared_libraries /usr/bin/xkbcomp
copy_shared_libraries /usr/bin/xrandr
copy_shared_libraries /usr/bin/curl
copy_shared_libraries /usr/bin/install
copy_shared_libraries /usr/bin/udevadm
copy_shared_libraries /bin/bash
copy_shared_libraries "$xorg"
for utility in setxkbmap xinput amixer; do
    if command -v "$utility" >/dev/null 2>&1; then
        cp "$(command -v "$utility")" "$stage/usr/bin/$utility"
        copy_shared_libraries "$(command -v "$utility")"
    fi
done
if test -d /usr/share/alsa; then
    mkdir -p "$stage/usr/share/alsa"
    # Base ALSA mixer profiles cover the emulated cards. Machine-specific UCM
    # profiles are not used by this VM and need a separate hardware package.
    for profile in alsa.conf cards pcm; do
        if test -e "/usr/share/alsa/$profile"; then cp -a "/usr/share/alsa/$profile" "$stage/usr/share/alsa/$profile"; fi
    done
fi
# Keep normal module names and dependency metadata so udev/modprobe can bind
# network, audio and common input devices instead of relying on host drivers.
for driver in virtio_net e1000 e1000e r8169 virtio_input hid_generic usbhid psmouse snd_hda_intel snd_intel8x0 snd_virtio vfat nls_cp437 nls_ascii nls_iso8859_1; do
    modprobe -S "$kernel_release" --show-depends "$driver" 2>/dev/null | while read -r directive module rest; do
        test "$directive" = insmod || continue
        destination="$stage${module%.zst}"
        mkdir -p "$(dirname -- "$destination")"
        case "$module" in *.zst) zstd -q -d -c "$module" >"$destination" ;; *) cp "$module" "$destination" ;; esac
    done
done
depmod -b "$stage" "$kernel_release" 2>/dev/null
for metadata in modules.builtin modules.builtin.modinfo; do
    if test -f "/lib/modules/$kernel_release/$metadata"; then cp "/lib/modules/$kernel_release/$metadata" "$stage/lib/modules/$kernel_release/$metadata"; fi
done
cat >"$stage/etc/passwd" <<'PASSWD'
root:x:0:0:Winlux:/root:/bin/sh
PASSWD
cat >"$stage/etc/group" <<'GROUP'
root:x:0:
audio:x:29:root
video:x:44:root
input:x:109:root
GROUP
cat >"$stage/etc/mdev.conf" <<'MDEV'
null 0:0 0666
zero 0:0 0666
full 0:0 0666
random 0:0 0666
urandom 0:0 0666
ptmx 0:0 0666
tty 0:0 0666
MDEV
cat >"$stage/etc/winlux/input.conf" <<'INPUT'
XKB_LAYOUT=fr
TAP_TO_CLICK=1
INPUT
cat >"$stage/etc/winlux/preferences.conf" <<'PREFS'
TASKBAR_ALIGN=left
THEME=dark
PREFS
cat >"$stage/etc/winlux/users.conf" <<'USERS'
root:root:winlux
USERS
cp -L "$kernel" "$stage/boot/vmlinuz"
printf '%s\n' "$kernel_release" >"$stage/usr/share/winlux/kernel-version"
# A visible, usable terminal is mandatory for desktop and console applications.
sh "$root/scripts/stage-terminal.sh" "$stage"
# Boîte d'erreur minimale pour les lancements sans bureau Linux complet.
if command -v xmessage >/dev/null 2>&1; then
    cp "$(command -v xmessage)" "$stage/usr/bin/xmessage"
    chmod 755 "$stage/usr/bin/xmessage"
    copy_shared_libraries "$(command -v xmessage)"
fi
# Microsoft Edge, navigateur par défaut : intégré seulement si un paquet
# .deb officiel est fourni (WINLUX_EDGE_DEB=/chemin/microsoft-edge-stable.deb,
# téléchargé depuis https://www.microsoft.com/edge). Sinon le bouton Edge
# affiche une notification.
if [ -n "${WINLUX_EDGE_DEB:-}" ]; then
    test -r "$WINLUX_EDGE_DEB" || { echo "paquet Edge illisible : $WINLUX_EDGE_DEB" >&2; exit 1; }
    dpkg-deb -x "$WINLUX_EDGE_DEB" "$stage"
    test -x "$stage/opt/microsoft/msedge/msedge" || { echo "paquet Edge inattendu" >&2; exit 1; }
    missing=$(ldd "$stage/opt/microsoft/msedge/msedge" | awk '/not found/ { print $1 }')
    if [ -n "$missing" ]; then
        echo "bibliothèques manquantes pour Edge sur cette machine : $missing" >&2
        echo "installez-les (sudo apt install microsoft-edge-stable les tire) puis relancez" >&2
        exit 1
    fi
    copy_shared_libraries "$stage/opt/microsoft/msedge/msedge"
    ln -sf /opt/microsoft/msedge/microsoft-edge "$stage/usr/bin/microsoft-edge-stable"
fi
# Offline browser catalogue; official packages are prepared before this build.
browser_packages=${WINLUX_BROWSER_PACKAGES:-"$root/build/browser-packages"}
if test -f "$browser_packages/edge.deb"; then
    sh "$root/scripts/stage-browser-packages.sh" "$stage" "$browser_packages"
fi
mkdir -p "$stage$(dirname -- "$xft_library")"
cp -L "$xft_library" "$stage$xft_library"
copy_shared_libraries "$xft_library"
mkdir -p "$stage$(dirname -- "$cairo_library")"
cp -L "$cairo_library" "$stage$cairo_library"
copy_shared_libraries "$cairo_library"
cp -a "$xorg_modules" "$stage/usr/lib/xorg/modules"
find "$xorg_modules" -type f -name '*.so' | while IFS= read -r module; do
    copy_shared_libraries "$module"
done
cp -a "$fonts" "$stage/usr/share/fonts/X11/misc"
cp -a "$xkb" "$stage/usr/share/X11/xkb"
mkdir -p "$stage/usr/share/X11/xorg.conf.d"
cp /usr/share/X11/xorg.conf.d/40-libinput.conf "$stage/usr/share/X11/xorg.conf.d/40-libinput.conf"
# Données de libinput (quirks) : sans elles, les périphériques de pointage
# absolus comme la tablette virtio sont mal reconnus.
if test -d /usr/share/libinput; then
    mkdir -p "$stage/usr/share"
    cp -a /usr/share/libinput "$stage/usr/share/libinput"
fi
# Curseur logiciel : il est dessiné dans l'image de l'écran, donc toujours
# visible dans une VM, alors que le curseur matériel virtio-gpu dépend de
# l'affichage de l'hôte (QEMU GTK, SPICE, VNC…).
cat >"$stage/usr/share/X11/xorg.conf.d/20-winlux-display.conf" <<'XCONF'
Section "Device"
    Identifier "Winlux display"
    Driver "modesetting"
    Option "SWcursor" "on"
EndSection
XCONF
cp -a "$truetype_fonts" "$stage/usr/share/fonts/truetype/dejavu"
cp -a /etc/fonts/. "$stage/etc/fonts/"
mkdir -p "$stage/etc/ssl/certs"
cp /etc/ssl/certs/ca-certificates.crt "$stage/etc/ssl/certs/ca-certificates.crt"
if test -d /usr/share/fontconfig; then
    cp -a /usr/share/fontconfig "$stage/usr/share/fontconfig"
fi
# Supply standard command locations too: third-party launchers may use
# /bin/readlink or /bin/cat rather than looking them up in PATH.
for applet in $(/usr/bin/busybox --list); do
    case "$applet" in '['|'[[') continue ;; esac
    if ! test -e "$stage/usr/bin/$applet"; then ln -s ../../bin/busybox "$stage/usr/bin/$applet"; fi
    if ! test -e "$stage/bin/$applet"; then ln -s busybox "$stage/bin/$applet"; fi
done

# Le shell ne copie que les icônes réellement utilisées dans la préversion VM.
for icon in preferences-desktop distributor-logo-linux-mint; do
    source=$(find -L /usr/share/icons /usr/share/pixmaps -type f -name "$icon.png" 2>/dev/null | \
        awk 'NR == 1 { fallback=$0 } /\/256(@2x)?\// { print; found=1; exit } /\/128(@2x)?\// { candidate=$0 } END { if (!found) print candidate ? candidate : fallback }' | head -1)
    if test -n "$source"; then
        cp "$source" "$stage/usr/share/pixmaps/$icon.png"
    fi
done
# --- Intégration binfmt/MIME/.desktop (tâches 9-10) ---
# Configuration binfmt : le noyau lance automatiquement winlux pour les .exe MZ.
mkdir -p "$stage/etc/binfmt.d"
cp "$root/integration/winlux-binfmt.conf" "$stage/etc/binfmt.d/winlux.conf"
# Type MIME pour les .exe.
mkdir -p "$stage/usr/share/mime/packages"
cp "$root/integration/x-ms-dos-executable.xml" "$stage/usr/share/mime/packages/winlux.xml"
# .desktop pour ouvrir les .exe depuis un gestionnaire de fichiers.
cp "$root/integration/winlux.desktop" "$stage/usr/share/applications/winlux.desktop"
cp "$root/integration/winlux-browser.desktop" "$stage/usr/share/applications/winlux-browser.desktop"
# Xsession et .desktop pour les display managers (installation sur disque).
cp "$root/scripts/xsession-winlux" "$stage/usr/share/winlux/xsession-winlux"
cp "$root/scripts/winlux-session.desktop" "$stage/usr/share/applications/winlux-session.desktop"
chmod 755 "$stage/usr/share/winlux/xsession-winlux"
# Thème : polices DejaVu déjà copiées ; on s'assure que fontconfig est à jour
# et qu'aucune dépendance propriétaire n'est requise (pas de Segoe UI, pas de
# polices Microsoft). Le runtime traduit les noms de police Windows vers DejaVu.
if command -v fc-cache >/dev/null 2>&1; then
    fc-cache -f "$stage/usr/share/fonts" 2>/dev/null || true
fi
sh "$root/scripts/stage-desktop-components.sh" "$stage"

cat >"$stage/init" <<'INIT'
#!/bin/sh
PATH=/bin:/usr/bin:/sbin:/usr/sbin
HOME=/root
XKB_CONFIG_ROOT=/usr/share/X11/xkb
export PATH HOME XKB_CONFIG_ROOT
umask 077

/bin/busybox mount -t proc proc /proc 2>/dev/null || true
/bin/busybox mount -t sysfs sysfs /sys 2>/dev/null || true
/bin/busybox mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
/bin/busybox chmod 666 /dev/null /dev/zero /dev/random /dev/urandom
/bin/busybox ln -sf /proc/self/fd /dev/fd
/bin/busybox ln -sf /proc/self/fd/0 /dev/stdin
/bin/busybox ln -sf /proc/self/fd/1 /dev/stdout
/bin/busybox ln -sf /proc/self/fd/2 /dev/stderr
/bin/busybox mkdir -p /dev/shm /dev/pts
/bin/busybox mount -t tmpfs -o mode=1777 tmpfs /dev/shm 2>/dev/null || true
/bin/busybox mount -t devpts devpts /dev/pts 2>/dev/null || true
/bin/busybox awk '$2 == "/run" {mounted=1} END {exit !mounted}' /proc/mounts || /bin/busybox mount -t tmpfs -o mode=0755 tmpfs /run
/bin/busybox mount -t tmpfs -o mode=1777 tmpfs /tmp 2>/dev/null || true
/bin/busybox mkdir -p /run/winlux
safe_power() {
    if /usr/bin/winlux-update --busy; then
        echo 'Installation en cours : arrêt/redémarrage différé. Réessayez après la fin.'
        return
    fi
    # Ask remaining background services to terminate before flushing disks.
    /bin/busybox kill -TERM -1 2>/dev/null || true
    /bin/busybox sleep 5
    /bin/busybox sync
    /bin/busybox mount -o remount,ro / 2>/dev/null || true
    /bin/busybox "$1" -f
}
trap 'safe_power poweroff' USR2
trap 'safe_power reboot' TERM
# Recovery uses static BusyBox before loading drivers/shared user services.
while ! /usr/bin/winlux-update --boot >/run/winlux/boot-update.log 2>&1; do
    echo 'Récupération système impossible. Bureau bloqué, journal : /run/winlux/boot-update.log'
    if /bin/busybox grep -qw 'winlux.native-smoke=1' /proc/cmdline; then
        /bin/busybox cat /run/winlux/boot-update.log
        /bin/busybox mkdir -p /var/lib/winlux
        /bin/busybox cp /run/winlux/boot-update.log /var/lib/winlux/native-boot-failure.log
        echo 'WINLUX_NATIVE_EXIT=1'
        /bin/busybox sync
        /bin/busybox poweroff -f
    fi
    /bin/sh
done
/usr/lib/systemd/systemd-udevd --daemon 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/virtio_dma_buf.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/virtio-gpu.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/virtio-input.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/hid.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/usbhid.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/hid-generic.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/serio-raw.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/psmouse.ko 2>/dev/null || true
for driver in virtio_net e1000 e1000e snd_hda_intel snd_intel8x0; do /bin/busybox modprobe "$driver" 2>/dev/null || true; done
/bin/busybox mdev -s 2>/dev/null || true
/usr/bin/udevadm trigger --action=add 2>/dev/null || true
/usr/bin/udevadm settle --timeout=3 2>/dev/null || true
echo "Winlux input devices detected:"
/bin/busybox cat /proc/bus/input/devices 2>/dev/null || true
/bin/busybox mdev -s 2>/dev/null || true
# Existing installations retain /etc, including an older mdev configuration.
/bin/busybox chmod 666 /dev/null /dev/zero /dev/random /dev/urandom /dev/ptmx /dev/tty
# Enregistrer binfmt pour les .exe (MZ) si le système de fichiers est monté.
if /bin/busybox mount -t binfmt_misc binfmt_misc /proc/sys/fs/binfmt_misc 2>/dev/null; then
    if test -r /etc/binfmt.d/winlux.conf; then
        /bin/busybox cat /etc/binfmt.d/winlux.conf > /proc/sys/fs/binfmt_misc/register 2>/dev/null || true
    fi
fi
/bin/busybox mkdir -p /tmp/.X11-unix /run /root/.local/share/winlux
/bin/busybox chmod 1777 /tmp
/bin/busybox rm -f /tmp/.X0-lock /tmp/.X11-unix/X0
/bin/busybox ip link set lo up
/bin/busybox mkdir -p /run/winlux /etc/winlux
/bin/busybox touch /etc/winlux/live-session
if ! /bin/busybox grep -q '^winlux-web:' /etc/passwd; then
    browser_uid=1001
    while /bin/busybox awk -F: -v n="$browser_uid" '$3 == n { found=1 } END { exit !found }' /etc/passwd /etc/group; do
        browser_uid=$((browser_uid + 1))
    done
    browser_gid=$(/bin/busybox awk -F: '$1 == "winlux-web" { print $3; exit }' /etc/group)
    if test -z "$browser_gid"; then
        browser_gid=$browser_uid
        printf 'winlux-web:x:%s:\n' "$browser_gid" >> /etc/group
    fi
    printf 'winlux-web:!:%s:%s:Winlux browser:/home/winlux-web:/bin/sh\n' "$browser_uid" "$browser_gid" >> /etc/passwd
fi
/bin/busybox mkdir -p /home/winlux-web
/bin/busybox chmod 755 /home
/bin/busybox chown winlux-web:winlux-web /home/winlux-web
/bin/busybox chmod 700 /home/winlux-web
if test -f /usr/share/winlux/browsers/edge.deb; then
    /usr/bin/winlux-browsers --install edge >/run/winlux/browser-setup.log 2>&1 || true
fi
echo "WINLUX_SYSTEM_VERSION=$(/bin/busybox cat /usr/share/winlux/version)"
/bin/busybox awk '$2 == "/" { print "WINLUX_ROOT_MODE=" $3 }' /proc/mounts
# BusyBox poweroff/reboot signal PID 1. A shell init must handle those signals
# explicitly so the graphical power button really shuts down the guest.
# Démarrer les services systemd activés par les paquets APT.
if test -x /usr/bin/systemctl; then /usr/bin/systemctl --boot-start; fi
if ! test -x /usr/sbin/NetworkManager; then /usr/bin/winlux-network start & fi
if /bin/busybox grep -qw 'winlux.native-smoke=1' /proc/cmdline; then
    result=0
    /usr/bin/python3 /opt/winlux/scripts/test-native-session-guest.py || result=$?
    echo "WINLUX_NATIVE_EXIT=$result"
    if test "$result" != 0; then
        /bin/busybox mkdir -p /var/lib/winlux/session-logs
        /bin/busybox cp /run/winlux/*.log /var/lib/winlux/session-logs/ 2>/dev/null || true
        /bin/busybox cat /run/winlux/native-session.log /run/winlux/window-manager.log /run/winlux/panel.log /run/winlux/screensaver.log /var/lib/winlux/native-programs.log 2>/dev/null || true
    fi
    for service in catalogue panel screensaver window-manager; do
        if test -f "/run/winlux/$service.pid"; then /bin/busybox kill "$(/bin/busybox cat "/run/winlux/$service.pid")" 2>/dev/null || true; fi
    done
    /bin/busybox killall xfdesktop mousepad xterm winlux msedge chrome firefox xfconfd dconf-service 2>/dev/null || true
    /bin/busybox sleep 1
    /bin/busybox killall Xvfb 2>/dev/null || true
    /bin/busybox sync
    /bin/busybox mount -o remount,ro / 2>/dev/null || true
    /bin/busybox poweroff -f
    while :; do /bin/busybox sleep 2; done
fi
if /bin/busybox grep -qw 'winlux.browser-smoke=1' /proc/cmdline; then
    result=0
    /bin/sh /opt/winlux/scripts/test-browsers-guest.sh || result=$?
    echo "WINLUX_BROWSERS_RESULT=$result"
    if test "$result" != 0; then
        /bin/busybox cat /tmp/*-dom.log /tmp/firefox-render.log /run/winlux/browser-setup.log 2>/dev/null || true
    fi
    /bin/busybox sync
    /bin/busybox poweroff -f
    while :; do /bin/busybox sleep 2; done
fi
if /bin/busybox grep -qw 'winlux.smoke=1' /proc/cmdline; then
    # An explicit diagnostic boot, only used with a disposable test disk.
    for attempt in $(/bin/busybox seq 1 20); do
        /bin/busybox ip route | /bin/busybox grep -q '^default ' && break
        /bin/busybox sleep 1
    done
    /bin/busybox ip -4 addr
    /bin/busybox ip route
    /bin/busybox cat /etc/resolv.conf 2>/dev/null || true
    network_status=1
    if /usr/bin/curl --fail --silent --show-error --connect-timeout 10 --max-time 20 https://api.github.com/ >/run/winlux/network-probe.json; then network_status=0; fi
    echo "WINLUX_NETWORK_HTTPS_EXIT=$network_status"
    count=$(/bin/busybox cat /etc/winlux/smoke-count 2>/dev/null || echo 0)
    case "$count" in ''|*[!0-9]*) count=0 ;; esac
    count=$((count + 1))
    echo "$count" >/etc/winlux/smoke-count
    echo "WINLUX_PERSISTENCE_COUNT=$count"
    /usr/bin/winlux-control status
    /bin/busybox cat /run/winlux/control.txt
    /bin/busybox sync
    /bin/busybox poweroff -f
    while :; do /bin/busybox sleep 2; done
fi
(
    while :; do
        /usr/bin/winlux-update --auto-run >>/run/winlux/auto-update.log 2>&1 || true
        /bin/busybox sleep 1800
    done
) &

if /bin/busybox grep -qw 'winlux.interactive=1' /proc/cmdline; then
    echo "Winlux desktop preview: starting Xorg"
    /usr/bin/Xorg :0 -noreset -nolisten tcp -logfile /tmp/Xorg.0.log >/tmp/xorg-console.log 2>&1 &
    xorg_pid=$!
    /bin/busybox sleep 3
    echo "Winlux Xorg input probe:"
    /bin/busybox grep -Ei 'libinput|input device|mouse|keyboard|error|cursor|config' /tmp/Xorg.0.log 2>/dev/null || true
    if ! /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; then
        /bin/busybox cat /tmp/Xorg.0.log /tmp/xorg-console.log >&2
        /bin/sh
    fi
    export DISPLAY=:0 XDG_CURRENT_DESKTOP=XFCE XDG_SESSION_DESKTOP=winlux XDG_MENU_PREFIX=xfce-
    export XDG_CONFIG_DIRS=/etc/xdg
    export XDG_DATA_DIRS="$HOME/.local/share/flatpak/exports/share:/var/lib/flatpak/exports/share:/usr/local/share:/usr/share"
    export XDG_RUNTIME_DIR=/run/user/0 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/winlux/session-bus
    export WINLUX_NATIVE_SHELL=1
    if test -d /usr/lib/locale/fr_FR.utf8; then export LANG=fr_FR.UTF-8; else export LANG=C.UTF-8; fi
    if ! /usr/bin/winlux-native-session >/run/winlux/native-session.log 2>&1; then
        /bin/busybox cat /run/winlux/native-session.log >&2
        /bin/sh
    fi
    (
        while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
            DISPLAY=:0 /usr/bin/winlux-control status 2>/dev/null || true
            /bin/busybox sleep 3
        done
    ) &
    # Quand la fenêtre de la VM change de taille, QEMU annonce un nouveau
    # mode préféré : on l'applique pour que le bureau suive la fenêtre.
    (
        while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
            if /bin/busybox grep -qw 'winlux.debug=1' /proc/cmdline; then
                modes=$(DISPLAY=:0 /usr/bin/xrandr --query 2>/dev/null | /bin/busybox grep -E '[*+]' | /bin/busybox tr -s ' ' | /bin/busybox tr '\n' ';')
                if test "$modes" != "${last_modes:-}"; then echo "Winlux xrandr: $modes"; last_modes=$modes; fi
            fi
            # Follow the preferred VM mode, or keep the user's fixed mode.
            DISPLAY=:0 /usr/bin/winlux-display sync 2>/dev/null || true
            /bin/busybox sleep 1
        done
    ) &
    while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
        keyboard_layout=$(/bin/busybox sed -n 's/^XKB_LAYOUT=//p' /etc/winlux/input.conf 2>/dev/null)
        case "$keyboard_layout" in fr|us) DISPLAY=:0 /usr/bin/setxkbmap "$keyboard_layout" 2>/dev/null || true ;; esac
        tapping=$(/bin/busybox sed -n 's/^TAP_TO_CLICK=//p' /etc/winlux/input.conf 2>/dev/null)
        case "$tapping" in 0|1)
            for device in $(DISPLAY=:0 /usr/bin/xinput --list --id-only 2>/dev/null); do
                DISPLAY=:0 /usr/bin/xinput set-prop "$device" 'libinput Tapping Enabled' "$tapping" 2>/dev/null || true
            done ;;
        esac
        taskbar_align=$(/bin/busybox sed -n 's/^TASKBAR_ALIGN=//p' /etc/winlux/preferences.conf 2>/dev/null)
        desktop_theme=$(/bin/busybox sed -n 's/^THEME=//p' /etc/winlux/preferences.conf 2>/dev/null)
        # Échelle d'interface choisie d'après la hauteur de l'écran au lancement.
        screen_height=$(DISPLAY=:0 /usr/bin/xrandr --query 2>/dev/null | /bin/busybox awk '/\*/ { split($1, size, "x"); print size[2]; exit }')
        ui_scale=1
        if test "${screen_height:-0}" -ge 2000; then ui_scale=2
        elif test "${screen_height:-0}" -ge 1400; then ui_scale=1.5
        elif test "${screen_height:-0}" -ge 1000; then ui_scale=1.25
        fi
        saved_scale=$(/bin/busybox sed -n 's/^UI_SCALE=//p' /etc/winlux/display.conf 2>/dev/null)
        case "$saved_scale" in 1|1.25|1.5|2) ui_scale=$saved_scale ;; esac
        input_diag=
        if /bin/busybox grep -qw 'winlux.debug=1' /proc/cmdline; then input_diag=1; fi
        tray_battery=0
        tray_battery_present=0
        for capacity in /sys/class/power_supply/BAT*/capacity; do
            if test -r "$capacity"; then tray_battery=$(/bin/busybox cat "$capacity"); tray_battery_present=1; break; fi
        done
        tray_network=0
        for state in /sys/class/net/*/operstate; do
            if test -r "$state" && test "$(/bin/busybox cat "$state")" = up; then tray_network=1; break; fi
        done
        export WINLUX_UI_SCALE="$ui_scale" WINLUX_THEME="$desktop_theme"
        /usr/bin/xfdesktop --disable-wm-check >/run/winlux/desktop.log 2>&1 &
        desktop_pid=$!
        echo "WINLUX_DESKTOP_READY pid=$desktop_pid"
        while /bin/busybox kill -0 "$desktop_pid" 2>/dev/null; do
            request=$(/bin/busybox cat /run/winlux/session-action 2>/dev/null)
            update=$(/bin/busybox cat /run/winlux/update-action 2>/dev/null)
            if test "$request" = restart-session && ! /usr/bin/winlux-update --busy; then
                /bin/busybox rm -f /run/winlux/session-action
                if test "$update" = restart-session; then /bin/busybox rm -f /run/winlux/update-action; fi
                /bin/busybox kill "$desktop_pid" 2>/dev/null || true
                break
            fi
            /bin/busybox sleep 1
        done
        wait "$desktop_pid" 2>/dev/null || true
        /bin/busybox sleep 1
    done
    /bin/sh
fi

echo "Winlux live test: headless PE suite"
WINLUX_BIN=/usr/bin/winlux \
WINLUX_TESTDIR=/opt/winlux/tests \
    /bin/sh /opt/winlux/scripts/test-runtime.sh
sequential_status=$?
echo "WINLUX_LIVE_SEQUENTIAL_EXIT=$sequential_status"

echo "Winlux live test: parallel PE suite"
WINLUX_BIN=/usr/bin/winlux \
WINLUX_TESTDIR=/opt/winlux/tests \
    /bin/sh /opt/winlux/scripts/test-runtime-parallel.sh
parallel_status=$?
echo "WINLUX_LIVE_PARALLEL_EXIT=$parallel_status"

echo "Winlux live test: Xvfb GUI rendering"
/usr/bin/Xvfb :99 -ac -screen 0 1920x1080x24 -nolisten tcp \
    -fp /usr/share/fonts/X11/misc,built-ins >/tmp/xvfb.log 2>&1 &
xvfb_pid=$!
/bin/busybox sleep 1
x11_status=1
terminal_status=1
if /bin/busybox kill -0 "$xvfb_pid" 2>/dev/null; then
    DISPLAY=:99 /bin/sh /opt/winlux/scripts/test-terminal-guest.sh
    terminal_status=$?
    if test "$terminal_status" != 0; then /bin/busybox cat /tmp/winlux-terminal.log >&2; fi
    DISPLAY=:99 WINLUX_TEST=salut WINLUX_CAPTURE_TEST=1 WINLUX_DIB_TEST=1 \
        /usr/bin/winlux /opt/winlux/tests/gui_demo.exe
    x11_status=$?
else
    /bin/busybox cat /tmp/xvfb.log >&2
fi
/bin/busybox kill "$xvfb_pid" 2>/dev/null || true
echo "WINLUX_LIVE_X11_EXIT=$x11_status"

result=0
if [ "$sequential_status" -ne 0 ] || \
   [ "$parallel_status" -ne 0 ] || \
   [ "$x11_status" -ne 0 ] || [ "$terminal_status" -ne 0 ]; then
    result=1
fi
echo "WINLUX_LIVE_RESULT=$result"
/bin/busybox sync
/bin/busybox poweroff -f
while :; do /bin/busybox sleep 2; done
INIT
chmod 755 "$stage/init"
"$root/scripts/source-fingerprint.sh" >"$stage/usr/share/winlux/source-fingerprint"
if test "${WINLUX_LEGACY_BASE:-0}" != 1; then
    python3 "$root/scripts/stage-apt-root.py" "$stage" "$apt_base" --package "$root/build/winlux-desktop.deb"
fi

mkdir -p "$iso_root/boot/grub"
cp -L "$kernel" "$iso_root/boot/vmlinuz"
sh "$root/scripts/build-live-root.sh" "$stage" "$iso_root" "$kernel_release"
cat >"$iso_root/boot/grub/grub.cfg" <<'GRUB'
set default=1
set timeout=2
set timeout_style=hidden
if serial --unit=0 --speed=115200; then
    terminal_input serial console
    terminal_output serial console
fi
menuentry "Winlux diagnostic (headless + Xvfb)" --hotkey=d {
    linux /boot/vmlinuz console=ttyS0,115200n8 rootfstype=ramfs rdinit=/init panic=1 loglevel=4
    initrd /boot/winlux-initramfs.cpio.gz
}
function winlux_live {
    linux /boot/vmlinuz console=tty1 console=ttyS0,115200n8 rootfstype=ramfs rdinit=/init panic=1 quiet loglevel=3 vt.global_cursor_default=0 winlux.interactive=1
    initrd /boot/winlux-initramfs.cpio.gz
}
menuentry "Winlux Desktop" {
    if search --no-floppy --label WINLUX_ROOT --set=winlux_root; then
        if test -f "($winlux_root)/boot/vmlinuz"; then
            # Load the updated kernel and persistent root from the disk.
            if test -f "($winlux_root)/boot/winlux-split-root"; then
                linux "($winlux_root)/boot/vmlinuz" root=LABEL=WINLUX_ROOT rw rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 panic=1 quiet loglevel=3 vt.global_cursor_default=0 winlux.interactive=1
                initrd "($winlux_root)/boot/winlux-initramfs.cpio.gz"
            else
                linux "($winlux_root)/boot/vmlinuz" root=/dev/vda rw rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 panic=1 quiet loglevel=3 vt.global_cursor_default=0 winlux.interactive=1
            fi
        else
            winlux_live
        fi
    else
        winlux_live
    fi
}
menuentry "Installer Winlux sur le disque" --hotkey=i {
    winlux_live
}
menuentry "Winlux persistent network diagnostic" --hotkey=s {
    search --no-floppy --label WINLUX_ROOT --set=winlux_root
    if test -f "($winlux_root)/boot/winlux-split-root"; then
        linux "($winlux_root)/boot/vmlinuz" root=LABEL=WINLUX_ROOT rw rootfstype=ext4 init=/init console=ttyS0,115200n8 panic=1 loglevel=3 winlux.smoke=1
        initrd "($winlux_root)/boot/winlux-initramfs.cpio.gz"
    else
        linux "($winlux_root)/boot/vmlinuz" root=/dev/vda rw rootfstype=ext4 init=/init console=ttyS0,115200n8 panic=1 loglevel=3 winlux.smoke=1
    fi
}
GRUB
grub-script-check "$iso_root/boot/grub/grub.cfg"

mkdir -p "$(dirname -- "$out")"
grub-mkrescue -o "$out" "$iso_root"
sha256sum "$out" >"$out.sha256"
echo "ISO live créée : $out"
echo "Somme SHA-256 : $out.sha256"
