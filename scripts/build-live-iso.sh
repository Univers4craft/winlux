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
# Trois chemins d'amorçage : BIOS, UEFI 64 bits, et UEFI 32 bits pour les
# processeurs 64 bits sur micrologiciel 32 bits (tablettes Atom).
for platform in i386-pc x86_64-efi i386-efi; do
    test -d "/usr/lib/grub/$platform" || {
        echo "modules GRUB $platform absents (paquets grub-pc-bin, grub-efi-amd64-bin, grub-efi-ia32-bin)" >&2
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
# Lecteurs réseau (FTP, SFTP, SMB, NFS, WebDAV, S3) : même binaire.
ln -sf winlux "$stage/usr/bin/winlux-drives"
cp "$root/scripts/winlux-update.sh" "$stage/usr/bin/winlux-update"
cp "$root/scripts/winlux-update-progress.py" "$stage/usr/bin/winlux-update-progress"
chmod 755 "$stage/usr/bin/winlux-update-progress"
cp "$root/scripts/winlux-control.sh" "$stage/usr/bin/winlux-control"
cp "$root/scripts/winlux-speak.sh" "$stage/usr/bin/winlux-speak"
chmod 755 "$stage/usr/bin/winlux-speak"
cp "$root/scripts/winlux-journal.sh" "$stage/usr/bin/winlux-journal"
cp "$root/scripts/winlux-restore.sh" "$stage/usr/bin/winlux-restore"
cp "$root/scripts/winlux-sysupdate.sh" "$stage/usr/bin/winlux-sysupdate"
chmod 755 "$stage/usr/bin/winlux-sysupdate"
cp "$root/scripts/winlux-network.sh" "$stage/usr/bin/winlux-network"
cp "$root/scripts/winlux-bluetooth.sh" "$stage/usr/bin/winlux-bluetooth"
chmod 755 "$stage/usr/bin/winlux-bluetooth"
cp "$root/scripts/winlux-display.sh" "$stage/usr/bin/winlux-display"
chmod 755 "$stage/usr/bin/winlux-display"
mkdir -p "$stage/usr/lib/winlux/sleep.d"
cp "$root/scripts/winlux-sleep-display.sh" "$stage/usr/lib/winlux/sleep.d/10-display"
chmod 755 "$stage/usr/lib/winlux/sleep.d/10-display"
cp "$root/scripts/winlux-touchpad.sh" "$stage/usr/bin/winlux-touchpad"
chmod 755 "$stage/usr/bin/winlux-touchpad"
cp "$root/scripts/winlux-sleep-touchpad.sh" "$stage/usr/lib/winlux/sleep.d/20-touchpad"
chmod 755 "$stage/usr/lib/winlux/sleep.d/20-touchpad"
cp "$root/scripts/winlux-prime-run.sh" "$stage/usr/bin/winlux-prime-run"
chmod 755 "$stage/usr/bin/winlux-prime-run"
# Historique des fichiers, sessions multi-utilisateur, bureau à distance
cp "$root/scripts/winlux-backup.sh" "$stage/usr/bin/winlux-backup"
cp "$root/scripts/winlux-sessions.sh" "$stage/usr/bin/winlux-sessions"
cp "$root/scripts/winlux-remote-config.sh" "$stage/usr/bin/winlux-remote-config"
cp "$root/target/release/winlux-remote" "$stage/usr/bin/winlux-remote"
chmod 755 "$stage/usr/bin/winlux-backup" "$stage/usr/bin/winlux-sessions" "$stage/usr/bin/winlux-remote-config" "$stage/usr/bin/winlux-remote"
# Partage de dossiers (Samba si présent) et transfert des réglages par fichier.
cp "$root/scripts/winlux-share.sh" "$stage/usr/bin/winlux-share"
cp "$root/scripts/winlux-transfer.sh" "$stage/usr/bin/winlux-transfer"
chmod 755 "$stage/usr/bin/winlux-share" "$stage/usr/bin/winlux-transfer"
mkdir -p "$stage/usr/share/winlux"
# Store de logiciels et gestionnaire de polices du shell.
cp "$root/scripts/winlux-store.py" "$stage/usr/bin/winlux-store"
cp "$root/scripts/winlux-fonts.sh" "$stage/usr/bin/winlux-fonts"
cp "$root/integration/store-catalog.txt" "$stage/usr/share/winlux/store-catalog.txt"
# Catalogues ajoutés à la main par l'administrateur (aucun livré).
mkdir -p "$stage/etc/winlux/store-sources.d"
# Boutiques tierces proposées (docs/BOUTIQUES.md) : seulement leur description ;
# rien n'est téléchargé avant le choix de l'utilisateur, aucune n'est cochée.
cp "$root/scripts/winlux-store-catalogs.py" "$stage/usr/bin/winlux-store-catalogs"
cp "$root/integration/store-catalogs.json" "$stage/usr/share/winlux/store-catalogs.json"
chmod 755 "$stage/usr/bin/winlux-store" "$stage/usr/bin/winlux-fonts" "$stage/usr/bin/winlux-store-catalogs"
# Claude Code (#255) : « winlux-claude-code install » télécharge et vérifie
# claude.exe après le premier démarrage ; rien de lourd dans l'ISO.
if test -f "$root/scripts/winlux-claude-code.sh"; then
    install -m 755 "$root/scripts/winlux-claude-code.sh" "$stage/usr/bin/winlux-claude-code"
fi
cp "$root/scripts/winlux-dhcp-hook.sh" "$stage/usr/share/winlux/udhcpc.script"
cp "$root/integration/winlux-browser" "$stage/usr/bin/winlux-browser"
cp "$root/scripts/winlux-browsers.sh" "$stage/usr/bin/winlux-browsers"
chmod 755 "$stage/usr/bin/winlux-browsers"
cp "$root/integration/winlux-open" "$stage/usr/bin/winlux-open"
cp "$root/scripts/winlux-hardware.sh" "$stage/usr/bin/winlux-hardware"
cp "$root/scripts/winlux-firmware.sh" "$stage/usr/bin/winlux-firmware"
# Pilotes du matériel réel (cartes graphiques, NVIDIA, VA-API, microcode, fwupd).
install -m 755 "$root/scripts/winlux-drivers.py" "$stage/usr/bin/winlux-drivers"
install -m 755 "$root/scripts/winlux-hardware-check.py" "$stage/usr/bin/winlux-hardware-check"
install -m 755 "$root/scripts/winlux-rotation.py" "$stage/usr/bin/winlux-rotation"
chmod 755 "$stage/usr/bin/winlux-browser" "$stage/usr/bin/winlux-open" "$stage/usr/bin/winlux-control" "$stage/usr/bin/winlux-journal" "$stage/usr/bin/winlux-restore" "$stage/usr/bin/winlux-network" "$stage/usr/share/winlux/udhcpc.script" "$stage/usr/bin/winlux-hardware" "$stage/usr/bin/winlux-firmware"

# Installateur et outils associés
cp "$root/scripts/winlux-installer.py" "$stage/usr/bin/winlux-installer"
cp "$root/scripts/winlux-hw-detect.sh" "$stage/usr/bin/winlux-hw-detect"
cp "$root/scripts/winlux-first-boot.sh" "$stage/usr/bin/winlux-first-boot"
cp "$root/scripts/install.sh" "$stage/opt/winlux/scripts/install.sh"
cp "$root/scripts/winlux-grub-theme.sh" "$stage/opt/winlux/scripts/winlux-grub-theme.sh"
cp "$root/scripts/install-safety.sh" "$stage/opt/winlux/scripts/install-safety.sh"
mkdir -p "$stage/usr/share/winlux"
cp "$root/scripts/winlux-recovery.sh" "$stage/usr/share/winlux/winlux-recovery.sh"
# Réinitialiser ce PC, clé de secours, réparation du démarrage (#283).
install -m 755 "$root/scripts/winlux-reset.py" "$stage/usr/bin/winlux-reset"
# Fichiers de comptes abîmés par un arrêt brutal : réparés par l'init.
install -m 755 "$root/scripts/winlux-accounts-repair.py" "$stage/usr/bin/winlux-accounts-repair"
# Écran de secours quand le bureau ne s'ouvre pas (au lieu d'un écran noir).
install -m 755 "$root/scripts/winlux-rescue-screen.py" "$stage/usr/bin/winlux-rescue-screen"
cp "$root/scripts/winlux_autoinstall.py" "$stage/usr/share/winlux/winlux_autoinstall.py"
cp "$root/scripts/winlux-secureboot.sh" "$stage/usr/bin/winlux-secureboot"
chmod 755 "$stage/usr/share/winlux/winlux-recovery.sh" "$stage/usr/bin/winlux-secureboot"
# The installer copies the signed boot chain to the target ESP from these paths.
if test "${WINLUX_SECUREBOOT:-0}" = 1; then
    mkdir -p "$stage/usr/lib/shim" "$stage/usr/lib/grub/x86_64-efi-signed"
    cp /usr/lib/shim/shimx64.efi.signed /usr/lib/shim/mmx64.efi "$stage/usr/lib/shim/"
    cp /usr/lib/grub/x86_64-efi-signed/*.efi.signed "$stage/usr/lib/grub/x86_64-efi-signed/"
fi
mkdir -p "$stage/usr/share/winlux/autoinstall"
cp "$root/integration/autoinstall/"*.yaml "$stage/usr/share/winlux/autoinstall/"
chmod 755 "$stage/usr/bin/winlux-installer" "$stage/usr/bin/winlux-hw-detect" "$stage/usr/bin/winlux-first-boot" "$stage/opt/winlux/scripts/install.sh"
mkdir -p "$stage/usr/lib/winlux/python"
for module in partitioner partition_ui screen benchmark profiles migrate; do
    cp "$root/scripts/winlux_$module.py" "$stage/usr/lib/winlux/python/winlux_$module.py"
done
for tool in benchmark:benchmark profile:profiles migrate:migrate; do
    printf '#!/bin/sh\nexec /usr/bin/python3 /usr/lib/winlux/python/winlux_%s.py "$@"\n' "${tool#*:}" >"$stage/usr/bin/winlux-${tool%%:*}"
    chmod 755 "$stage/usr/bin/winlux-${tool%%:*}"
done
cp "$root/integration/winlux-installer.desktop" "$stage/usr/share/applications/winlux-installer.desktop"
cp "$root/integration/winlux-migrate.desktop" "$stage/usr/share/applications/winlux-migrate.desktop"
mkdir -p "$stage/root/Bureau"
cp "$root/integration/winlux-installer.desktop" "$stage/root/Bureau/winlux-installer.desktop"
mkdir -p "$stage/usr/share/winlux"
printf '%s\n' "${WINLUX_BUILD_VERSION:-$(cat "$root/VERSION")}" >"$stage/usr/share/winlux/version"
sh "$root/scripts/build-info.sh" "$(cat "$stage/usr/share/winlux/version")" system >"$stage/usr/share/winlux/BUILD-INFO"
printf 'system\n' >"$stage/usr/share/winlux/update-scope"
mkdir -p "$stage/usr/share/winlux/avatars"
cp "$root"/assets/avatars/avatar-*.svg "$stage/usr/share/winlux/avatars/"
# Identité visuelle : logo, fonds Winlux et thème GRUB repris par l'installateur.
mkdir -p "$stage/usr/share/winlux/brand" "$stage/usr/share/winlux/wallpapers"
cp "$root"/assets/brand/winlux-logo*.svg "$root"/assets/brand/winlux-logo-*.png "$stage/usr/share/winlux/brand/"
cp "$root"/assets/wallpapers/winlux-prisme-*.png "$stage/usr/share/winlux/wallpapers/"
sh "$root/scripts/winlux-grub-theme.sh" install "$root/assets/brand/grub" "$stage/usr/share/winlux/grub-theme"

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
cp "$root/scripts/run-tls-suite.sh" "$stage/opt/winlux/scripts/run-tls-suite.sh"
cp "$root/scripts/test-terminal-guest.sh" "$stage/opt/winlux/scripts/test-terminal-guest.sh"
cp "$root/scripts/test-native-session-guest.py" "$stage/opt/winlux/scripts/test-native-session-guest.py"
cp "$root/scripts/test-apt-native-guest.py" "$stage/opt/winlux/scripts/test-apt-native-guest.py"
cp "$root/scripts/test-browsers-guest.sh" "$stage/opt/winlux/scripts/test-browsers-guest.sh"
cp "$root/scripts/test-runtime-parallel.sh" "$stage/opt/winlux/scripts/test-runtime-parallel.sh"
chmod 755 \
    "$stage/usr/bin/winlux" "$stage/usr/bin/winlux-update" \
    "$stage/opt/winlux/scripts/test-runtime.sh" \
    "$stage/opt/winlux/scripts/run-tls-suite.sh" \
    "$stage/opt/winlux/scripts/test-runtime-parallel.sh"
for name in hello exitcode p1_suite dll_import delay_import findfiles crt_suite gui_suite controls_suite gui_demo explorer_regression imageviewer voicerecorder power_api audio_suite d3d_suite gl_vk_suite system32_suite tls_suite print_suite winpaths uac_suite comctl_suite bars_suite commdlg_suite menu_suite; do
    cp "$root/target/x86_64-pc-windows-msvc/release/$name.exe" "$stage/opt/winlux/tests/$name.exe"
done
cp "$root/target/x86_64-pc-windows-msvc/release/testdll.dll" "$stage/opt/winlux/tests/testdll.dll"
# Tests C MinGW, dont games_suite (fonctions des vrais jeux, msvcrt et UCRT),
# quand le compilateur MinGW-w64 est présent sur la machine de construction.
if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    sh "$root/scripts/build-mingw-tests.sh"
    for name in mingw_suite games_suite games_suite_ucrt; do
        cp "$root/target/mingw/$name.exe" "$stage/opt/winlux/tests/$name.exe"
    done
fi

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
for applet in awk basename cat chmod chown cmp cp cut date env find grep head id ip ipcalc kill killall ln ls mkdir modprobe mount mountpoint mv od printf ps pwd readlink reboot poweroff rm rmdir sed sha256sum sleep sort stat sync tar tee touch tr truncate umount uname wc xargs; do
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
copy_shared_libraries "$root/target/release/winlux-remote"
sh "$root/scripts/copy-audio-libraries.sh" "$stage"
sh "$root/scripts/stage-media.sh" "$stage" "$root"
copy_shared_libraries /usr/bin/Xvfb
copy_shared_libraries /usr/bin/xkbcomp
copy_shared_libraries /usr/bin/xrandr
copy_shared_libraries /usr/bin/curl
# Enregistrement d'écran (Win+Maj+R) : encodeur ffmpeg du système, facultatif.
ffmpeg_bin=${WINLUX_FFMPEG:-$(command -v ffmpeg || true)}
if test -n "$ffmpeg_bin" && test -x "$ffmpeg_bin"; then
    cp "$ffmpeg_bin" "$stage/usr/bin/ffmpeg"
    chmod 755 "$stage/usr/bin/ffmpeg"
    copy_shared_libraries "$ffmpeg_bin"
else
    echo "ffmpeg absent : l'enregistrement vidéo de l'écran sera indisponible dans l'ISO" >&2
fi
copy_shared_libraries /usr/bin/install
copy_shared_libraries /usr/bin/udevadm
copy_shared_libraries /bin/bash
copy_shared_libraries "$xorg"
for utility in setxkbmap xinput amixer fc-list fc-cache; do
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
# Display drivers of the usual VM adapters (QEMU std VGA, QXL, VMware and
# VirtualBox VMSVGA): without a DRM device the modesetting driver of Xorg has
# nothing to drive and the desktop never appears (virtio-gpu is loaded apart).
# nls_utf8: FAT and exFAT USB keys are mounted with iocharset=utf8 (volumes.rs);
# without it every such key failed with "IO charset utf8 not found". usb_storage
# and uas give USB keys their /dev/sdX even where the kernel builds them as modules.
# Matériel réel : scripts/hardware-modules.list (cartes graphiques, Wi-Fi,
# Ethernet, Bluetooth, son, pavés tactiles, capteurs). udev ne les charge que
# si le matériel est là.
hardware_modules=$(sed 's/#.*//' "$root/scripts/hardware-modules.list" | tr '\n' ' ')
for driver in virtio_net e1000 e1000e r8169 virtio_input hid_generic usbhid psmouse snd_hda_intel snd_intel8x0 snd_virtio vfat nls_cp437 nls_ascii nls_iso8859_1 nls_utf8 usb_storage uas bochs qxl vmwgfx cifs nfs nfsv3 nfsv4 $hardware_modules; do
    modprobe -S "$kernel_release" --show-depends "$driver" 2>/dev/null | while read -r directive module rest; do
        test "$directive" = insmod || continue
        destination="$stage${module%.zst}"
        mkdir -p "$(dirname -- "$destination")"
        case "$module" in *.zst) zstd -q -d -c "$module" >"$destination" ;; *) cp "$module" "$destination" ;; esac
    done
done
# Son : tous les pilotes ALSA/ASoC et SoundWire. Les cartes des portables
# récents (SOF, AMD ACP) chargent leur pilote de machine et leurs codecs par
# leur nom au démarrage de la carte, sans dépendance déclarée : sans eux, la
# carte ne se crée pas et PipeWire n'offre qu'une sortie factice.
# Wi-Fi : la pile 802.11 entière, tous les pilotes Wi-Fi (PCIe, USB, SDIO des
# tablettes, staging dont r8723bs des Bay Trail/Cherry Trail) et la crypto du
# noyau. mac80211 demande ccm(aes) par son nom pour poser la clé CCMP de tout
# réseau WPA2/WPA3 (cmac : trames de gestion protégées, michael_mic : TKIP,
# gcm/ghash : GCMP) ; sans ces modules la poignée de main échouait et le mot
# de passe Wi-Fi semblait refusé (v0.2.17, PC de damien). brcmfmac charge de
# même ses modules « vendeur » (cyw, wcc, bca) par leur nom.
# Pare-feu et partage de connexion : netfilter entier (nf_tables, NAT,
# conntrack), pont et veth ; sans eux winlux-firewall ne faisait rien
# (« Unable to initialize Netlink socket ») et aucun NAT n'était possible.
# Partage de connexion d'un téléphone : USB (cdc_ether, rndis_host, cdc_ncm,
# ipheth pour l'iPhone) et Bluetooth (bnep) sont dans drivers/net/usb et
# net/bluetooth.
for directory in sound drivers/soundwire crypto arch/x86/crypto lib/crypto net/wireless net/mac80211 drivers/net/wireless drivers/staging net/netfilter net/ipv4/netfilter net/ipv6/netfilter net/bridge drivers/net/usb net/bluetooth; do
    test -d "/lib/modules/$kernel_release/kernel/$directory" || continue
    find "/lib/modules/$kernel_release/kernel/$directory" -name '*.ko*' | while read -r module; do
        destination="$stage${module%.zst}"
        test -e "$destination" && continue
        mkdir -p "$(dirname -- "$destination")"
        case "$module" in *.zst) zstd -q -d -c "$module" >"$destination" ;; *) cp "$module" "$destination" ;; esac
    done
done
sh "$root/scripts/stage-storage-tools.sh" "$stage" "$kernel_release"
# Matériel réel : dossiers entiers de pilotes (stockage, Ethernet, USB,
# Bluetooth, pavés et écrans tactiles, capteurs, portables, webcams, systèmes
# de fichiers, pare-feu…), dépendances comprises, compressés comme chez Ubuntu.
# Politique et coût : docs/MATERIEL.md ; contrôle : check-image-hardware.py.
python3 "$root/scripts/stage-kernel-modules.py" "$stage" "$kernel_release" "$root/scripts/hardware-module-dirs.list"
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
# Aucun navigateur n'est intégré à l'ISO : l'utilisateur choisit le sien.
# Microsoft Edge et Google Chrome ne pourraient de toute façon pas l'être :
# leurs licences interdisent de les redistribuer (Edge : « you will not ... share,
# publish, distribute, or lend the Software », section 2.8 f). Ils se
# téléchargent chez leur éditeur, sur le PC, à la demande de l'utilisateur.
if [ -n "${WINLUX_EDGE_DEB:-}" ]; then
    echo "WINLUX_EDGE_DEB refusé : Edge ne peut pas être redistribué dans l'ISO" >&2
    exit 1
fi
# Aucun navigateur dans l'image : seulement les outils et bibliothèques qui
# font tourner celui que l'utilisateur choisira (stage-browser-packages.sh
# refuse tout paquet de navigateur).
browser_packages=${WINLUX_BROWSER_PACKAGES:-"$root/build/browser-packages"}
mkdir -p "$browser_packages"
sh "$root/scripts/stage-browser-packages.sh" "$stage" "$browser_packages"
# Téléchargement du navigateur choisi chez son éditeur (index HTTPS, SHA-256).
install -D -m 755 "$root/scripts/prepare-browser-packages.py" "$stage/usr/lib/winlux/winlux-browser-fetch"
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
# Curseur logiciel : configuration de base pour VM. Sur matériel réel,
# winlux-hardware remplace ce fichier par une configuration GPU appropriée.
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
# Manettes lisibles par hid.dll (#271) : /dev/hidraw* des fabricants de jeu.
mkdir -p "$stage/etc/udev/rules.d"
cp "$root/integration/70-winlux-manettes.rules" "$stage/etc/udev/rules.d/70-winlux-manettes.rules"
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
# Wi-Fi chiffré : la construction échoue si ce qui l'a cassé en v0.2.17 manque.
sh "$root/scripts/check-image-wifi.sh" "$stage" "$kernel_release"

cat >"$stage/init" <<'INIT'
#!/bin/sh
PATH=/bin:/usr/bin:/sbin:/usr/sbin
HOME=/root
XKB_CONFIG_ROOT=/usr/share/X11/xkb
export PATH HOME XKB_CONFIG_ROOT
umask 077

/bin/busybox mount -t proc proc /proc 2>/dev/null || true
/bin/busybox mount -t sysfs sysfs /sys 2>/dev/null || true
# Variables UEFI (efibootmgr, entrée « Winlux » de l'installateur) : sans
# efivarfs, efibootmgr répond « EFI variables are not supported ». Échoue
# sans bruit avec efi=noruntime (UEFI 32 bits) ou au démarrage BIOS.
if test -d /sys/firmware/efi/efivars && ! /bin/busybox grep -q ' /sys/firmware/efi/efivars ' /proc/mounts; then
    /bin/busybox mount -t efivarfs efivarfs /sys/firmware/efi/efivars 2>/dev/null || true
fi
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
# Temps de démarrage : chaque étape note l'instant écoulé depuis l'allumage du
# noyau (Paramètres > Processeur, `winlux --boot-time`).
boot_mark() {
    read -r boot_uptime boot_idle </proc/uptime 2>/dev/null || return 0
    echo "$1 $boot_uptime" >>/run/winlux/boot-times
}
/bin/busybox rm -f /run/winlux/boot-times /run/winlux/boot-recorded
boot_mark init
# « Redémarrer sur le menu de secours » (Paramètres > Récupération) ne vaut
# que pour un démarrage, même si l'on a choisi le bureau dans le menu GRUB.
/bin/busybox rm -f /boot/winlux-next-recovery 2>/dev/null || true
# Écran Prisme dans le framebuffer jusqu'à l'arrivée du bureau (#170).
if /bin/busybox grep -qw 'winlux.interactive=1' /proc/cmdline && ! /bin/busybox grep -qw 'winlux.check-iso=1' /proc/cmdline && test -e /dev/fb0 && test -x /usr/bin/winlux; then
    /usr/bin/winlux --boot-splash >/run/winlux/boot-splash.log 2>&1 &
fi
safe_power() {
    if /usr/bin/winlux-update --busy; then
        echo 'Installation en cours : arrêt/redémarrage différé. Réessayez après la fin.'
        return
    fi
    # Ask remaining background services to terminate before flushing disks.
    echo "Winlux : arrêt des programmes ($1)"
    /bin/busybox kill -TERM -1 2>/dev/null || true
    # Garde-fou : si l'écriture des disques, le remontage ou l'appel au noyau
    # restent bloqués (session live juste après une installation : la
    # machine restait sur le logo et il fallait forcer l'arrêt), le noyau
    # termine lui-même au bout de 25 s : synchronisation, disques en lecture
    # seule, puis redémarrage (b) ou arrêt (o).
    sysrq_key=b
    test "$1" = poweroff && sysrq_key=o
    (
        /bin/busybox sleep 25
        echo "Winlux : arrêt forcé par le noyau ($1)"
        echo s >/proc/sysrq-trigger
        /bin/busybox sleep 2
        echo u >/proc/sysrq-trigger
        /bin/busybox sleep 1
        echo "$sysrq_key" >/proc/sysrq-trigger
    ) &
    /bin/busybox sleep 5
    echo "Winlux : écriture des disques"
    /bin/busybox sync
    /bin/busybox mount -o remount,ro / 2>/dev/null || true
    echo "Winlux : $1"
    /bin/busybox "$1" -f
}
trap 'safe_power poweroff' USR2
trap 'safe_power reboot' TERM
# Recovery uses static BusyBox before loading drivers/shared user services.
# umask 022 : les fichiers posés par dpkg et les caches restent lisibles.
while ! (umask 022; /usr/bin/winlux-update --boot) >/run/winlux/boot-update.log 2>&1; do
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
boot_mark recovery
/usr/lib/systemd/systemd-udevd --daemon 2>/dev/null || true
kernel_modules=/lib/modules/$(/bin/busybox uname -r)
/bin/busybox insmod "$kernel_modules"/virtio_dma_buf.ko 2>/dev/null || true
/bin/busybox insmod "$kernel_modules"/virtio-gpu.ko 2>/dev/null || true
/bin/busybox insmod "$kernel_modules"/virtio-input.ko 2>/dev/null || true
/bin/busybox insmod "$kernel_modules"/hid.ko 2>/dev/null || true
/bin/busybox insmod "$kernel_modules"/usbhid.ko 2>/dev/null || true
/bin/busybox insmod "$kernel_modules"/hid-generic.ko 2>/dev/null || true
/bin/busybox insmod "$kernel_modules"/serio-raw.ko 2>/dev/null || true
/bin/busybox insmod "$kernel_modules"/psmouse.ko 2>/dev/null || true
# Pilotes des cartes PCI présentes (réseau, son, affichage bochs/qxl/vmwgfx
# des VM…) en un seul appel, d'après leur modalias, au lieu de 13 modprobe à
# l'aveugle ; udev complète ensuite pour l'USB et le reste.
if test -x /usr/sbin/modprobe; then
    /bin/busybox cat /sys/bus/pci/devices/*/modalias 2>/dev/null | /bin/busybox xargs /usr/sbin/modprobe -a -b -q 2>/dev/null || true
else
    for driver in virtio_net e1000 e1000e r8169 igb igc snd_hda_intel snd_intel8x0 snd_hda_codec_realtek snd_usb_audio bochs qxl vmwgfx; do /bin/busybox modprobe "$driver" 2>/dev/null || true; done
fi
/bin/busybox mdev -s 2>/dev/null || true
/usr/bin/udevadm trigger --action=add 2>/dev/null || true
/usr/bin/udevadm settle --timeout=3 2>/dev/null || true
boot_mark drivers
# Partitions supplémentaires choisies à l'installation (/home, swap…).
if test -s /etc/fstab; then
    if test -x /usr/bin/mount; then /usr/bin/mount -a 2>/dev/null || true
    else /bin/busybox mount -a 2>/dev/null || true; fi
    /sbin/swapon -a 2>/dev/null || /bin/busybox swapon -a 2>/dev/null || true
fi
# Système installé : le bureau s'ouvre sous le compte créé à l'installation,
# plus en root. Seuls les services système restent en root ; le bureau passe
# par le relais winlux-root pour ce qui touche au système. winlux.root-session=1
# (ligne du noyau) rouvre l'ancien bureau root pour un dépannage.
# Comptes perdus ou à moitié créés après une coupure (octets nuls dans
# /etc/passwd, verrous orphelins) : réparés avant de chercher le compte.
if test -s /etc/winlux/install-config.json && test -x /usr/bin/winlux-accounts-repair; then
    /usr/bin/winlux-accounts-repair >/run/winlux/accounts-repair.log 2>&1 || true
fi
# Fuseau jamais choisi (installateur ≤ v0.2.18 : « timezone » vide, système
# resté en UTC, une heure de retard à Paris) : celui de la langue installée.
if test -s /etc/winlux/install-config.json && ! test -s /etc/timezone; then
    tz_locale=$(/bin/busybox sed -n 's/^LANG=\([a-zA-Z_]*\).*/\1/p' /etc/default/locale 2>/dev/null | /bin/busybox head -n 1)
    case "$tz_locale" in
        fr_BE) tz=Europe/Brussels ;; fr_CH|de_CH) tz=Europe/Zurich ;; fr_CA|en_CA) tz=America/Toronto ;;
        fr_LU) tz=Europe/Luxembourg ;; fr*) tz=Europe/Paris ;; de_AT) tz=Europe/Vienna ;; de*) tz=Europe/Berlin ;;
        es_MX) tz=America/Mexico_City ;; es*) tz=Europe/Madrid ;; en_US) tz=America/New_York ;; en*) tz=Europe/London ;;
        it*) tz=Europe/Rome ;; pt_BR) tz=America/Sao_Paulo ;; pt*) tz=Europe/Lisbon ;; *) tz= ;;
    esac
    if test -n "$tz" && test -f "/usr/share/zoneinfo/$tz"; then
        /bin/busybox ln -sfn "/usr/share/zoneinfo/$tz" /etc/localtime
        echo "$tz" >/etc/timezone
        /bin/busybox chmod 644 /etc/timezone
    fi
fi
# policy-rc.d de construction resté dans les systèmes installés en v0.2.18 :
# retiré, sinon aucun service d'un paquet installé ensuite ne démarre.
if test -f /usr/sbin/policy-rc.d && test "$(/bin/busybox cat /usr/sbin/policy-rc.d)" = "$(printf '#!/bin/sh\nexit 101')"; then
    /bin/busybox rm -f /usr/sbin/policy-rc.d
fi
desktop_user=
desktop_autologin=1
if test -s /etc/winlux/install-config.json && ! /bin/busybox grep -qw 'winlux.root-session=1' /proc/cmdline; then
    desktop_user=$(/bin/busybox sed -n 's/^[[:space:]]*"username":[[:space:]]*"\([a-z_][a-z0-9_.-]*\)".*/\1/p' /etc/winlux/install-config.json | /bin/busybox head -n 1)
    if test -n "$desktop_user" && ! /bin/busybox awk -F: -v u="$desktop_user" '$1 == u && $3 >= 1000 && $3 < 60000 { found=1 } END { exit !found }' /etc/passwd; then
        desktop_user=
    fi
    if /bin/busybox grep -q '"autologin":[[:space:]]*false' /etc/winlux/install-config.json; then desktop_autologin=0; fi
fi
if test -n "$desktop_user"; then
    # Fichiers d'état et réglages lisibles par le bureau (il n'est plus root).
    umask 022
    /bin/busybox chmod -R a+rX /run/winlux 2>/dev/null || true
    /bin/busybox rm -f /etc/winlux/live-session
    # Comptes de l'écran de verrouillage et des partages : les vrais comptes,
    # sans mot de passe en clair (« shadow » : vérifié par winlux-root).
    /bin/busybox awk -F: 'NR == FNR { if ($1 == "sudo" || $1 == "wheel" || $1 == "admin") { n = split($4, m, ","); for (i = 1; i <= n; i++) admin[m[i]] = 1 } next }
        $3 >= 1000 && $3 < 60000 && $1 != "winlux-web" && $7 !~ /(nologin|false)$/ { print $1 ":" (($1 in admin) ? "root" : "user") ":shadow" }' \
        /etc/group /etc/passwd >/etc/winlux/users.conf.new 2>/dev/null || true
    if /bin/busybox grep -q "^$desktop_user:" /etc/winlux/users.conf.new 2>/dev/null; then
        /bin/busybox chmod 644 /etc/winlux/users.conf.new
        /bin/busybox mv /etc/winlux/users.conf.new /etc/winlux/users.conf
    else
        /bin/busybox rm -f /etc/winlux/users.conf.new
    fi
    /bin/busybox chmod 755 /etc/winlux
    # Anciennes installations : caches MIME et état des mises à jour écrits
    # en umask 077 quand tout tournait en root.
    /bin/busybox chmod -R go+rX /usr/share/mime 2>/dev/null || true
    /bin/busybox chmod go+r /usr/share/applications/mimeinfo.cache 2>/dev/null || true
    /bin/busybox chmod go+rx /var/lib/winlux 2>/dev/null || true
    for conf in locale.conf input.conf preferences.conf cpu.conf overlay.conf lock.conf privacy.conf power.conf display.conf \
        settings.conf update.conf firewall.conf volumes.conf sysupdate.conf report.conf browser.conf remote.conf \
        profile.json install-config.json recommended-apps.txt benchmark.json hardware-report.txt; do
        if test -f "/etc/winlux/$conf"; then /bin/busybox chmod 644 "/etc/winlux/$conf"; fi
    done
fi
# Seule la partie utile à l'affichage (pilote GPU, configuration Xorg)
# précède Xorg ; le reste de l'inventaire, le contrôle des firmwares et le
# profil d'utilisation (sysctl, gouverneur CPU, E/S) suivent en arrière-plan.
/usr/bin/winlux-hardware display >/dev/null 2>&1 || true
(
    /usr/bin/winlux-hardware rest >/dev/null 2>&1 || true
    /usr/bin/winlux-firmware >/dev/null 2>&1 || true
    # Système installé : microcode et pilotes suivis à chaque démarrage, puis
    # au premier démarrage les pilotes recommandés (NVIDIA, pilotes complets
    # du noyau, VA-API, microcode), réseau venu et en basse priorité.
    if test -x /usr/bin/winlux-drivers && test -f /etc/winlux/install-config.json; then
        /usr/bin/winlux-drivers boot >/run/winlux/drivers-boot.log 2>&1 || true
        if test ! -e /var/lib/winlux/drivers/auto.done; then
            low=; test -x /usr/bin/nice && low='/usr/bin/nice -n 19'
            $low /usr/bin/winlux-drivers auto >>/var/log/winlux-drivers.log 2>&1 || true
        fi
    fi
    if test -x /usr/bin/winlux-profile && test -f /etc/winlux/profile.json; then
        /usr/bin/winlux-profile --apply-boot >/run/winlux/profile.log 2>&1 || true
    fi
) &
boot_mark hardware
echo "Winlux input devices detected:"
/bin/busybox cat /proc/bus/input/devices 2>/dev/null || true
/bin/busybox mdev -s 2>/dev/null || true
# Existing installations retain /etc, including an older mdev configuration.
/bin/busybox chmod 666 /dev/null /dev/zero /dev/random /dev/urandom /dev/ptmx /dev/tty
# Bureau sous le compte : mdev -s remet les périphériques en root:root 660
# et il n'y a pas de logind pour donner l'accès au siège. Son, carte
# graphique, webcam et manettes reviennent aux groupes du compte.
if test -n "$desktop_user"; then
    for node in /dev/snd/*; do test -c "$node" && /bin/busybox chgrp audio "$node" && /bin/busybox chmod 660 "$node"; done 2>/dev/null
    for node in /dev/dri/card* /dev/video* /dev/media*; do test -c "$node" && /bin/busybox chgrp video "$node" && /bin/busybox chmod 660 "$node"; done 2>/dev/null
    for node in /dev/dri/renderD* /dev/kfd; do test -c "$node" && /bin/busybox chmod 666 "$node"; done 2>/dev/null
    for node in /dev/input/js* /dev/input/event*; do test -c "$node" && /bin/busybox chgrp input "$node" && /bin/busybox chmod 660 "$node"; done 2>/dev/null
fi
# Enregistrer binfmt pour les .exe (MZ) : « ./programme.exe » dans un
# terminal lance winlux. Une règle par écriture et sans les commentaires du
# fichier : le noyau refusait tout le bloc, donc rien n'était enregistré en
# Live (#214). Le système de fichiers est peut-être déjà monté.
# binfmt : début
binfmt=/proc/sys/fs/binfmt_misc
binfmt_conf=/etc/binfmt.d/winlux.conf
test -e "$binfmt/register" || /bin/busybox mount -t binfmt_misc binfmt_misc "$binfmt" 2>/dev/null || true
if test -e "$binfmt/register" && test -r "$binfmt_conf"; then
    /bin/busybox grep '^:' "$binfmt_conf" | while IFS= read -r rule; do
        rule_name=${rule#:}
        rule_name=${rule_name%%:*}
        test -e "$binfmt/$rule_name" || printf '%s\n' "$rule" > "$binfmt/register" 2>/dev/null || true
    done
fi
# binfmt : fin
/bin/busybox mkdir -p /tmp/.X11-unix /run /root/.local/share/winlux
/bin/busybox chmod 1777 /tmp
/bin/busybox rm -f /tmp/.X0-lock /tmp/.X11-unix/X0
/bin/busybox ip link set lo up
/bin/busybox mkdir -p /run/winlux /etc/winlux
if test -n "$desktop_user"; then
    # Dossier d'état partagé : chacun y écrit ses fichiers, personne ne
    # touche à ceux des autres (sticky), root ne suit pas les liens d'autrui.
    /bin/busybox chmod 1777 /run/winlux
    # Journal Winlux : les .exe du compte y écrivent winlux.log et crash.log.
    # Créés ici avant tout service root (winlux --power daemon les créait en
    # 0644 root) : sinon un plantage sous le compte n'était jamais enregistré,
    # ni proposé au signalement, ni visible dans l'Observateur d'événements.
    /bin/busybox mkdir -p /run/winlux/journal
    /bin/busybox chmod 1777 /run/winlux/journal
    for journal_file in winlux.log crash.log; do
        /bin/busybox touch "/run/winlux/journal/$journal_file"
        /bin/busybox chmod 0666 "/run/winlux/journal/$journal_file"
    done
    echo 1 >/proc/sys/fs/protected_symlinks 2>/dev/null || true
    echo 1 >/proc/sys/fs/protected_hardlinks 2>/dev/null || true
else
    /bin/busybox touch /etc/winlux/live-session
fi
# Alimentation : profil, luminosite, veille sur inactivite, capot, bouton
# d'alimentation et batterie critique. Remplace logind/upower, sans systemd.
/usr/bin/winlux --power daemon >/run/winlux/power-daemon.log 2>&1 &
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
# Aucun navigateur à installer au démarrage : l'utilisateur choisit le sien.
browser_setup_pid=
echo "WINLUX_SYSTEM_VERSION=$(/bin/busybox cat /usr/share/winlux/version)"
/bin/busybox awk '$2 == "/" { print "WINLUX_ROOT_MODE=" $3 }' /proc/mounts
# BusyBox poweroff/reboot signal PID 1. A shell init must handle those signals
# explicitly so the graphical power button really shuts down the guest.
# Démarrer les services systemd activés par les paquets APT.
if test -x /usr/bin/systemctl; then /usr/bin/systemctl --boot-start; fi
if ! test -x /usr/sbin/NetworkManager; then /usr/bin/winlux-network start & fi
# Services maison : pare-feu (nftables, journal des blocages, alertes) et
# planificateur de tâches (déclencheurs horaires, démarrage, événements).
/usr/bin/winlux --firewall daemon >>/run/winlux/firewall.log 2>&1 &
# Disques branchés (clé USB, NTFS d'un Windows) : montage avec une lettre.
/usr/bin/winlux --volumes daemon >>/run/winlux/volumes.log 2>&1 &
/usr/bin/winlux --tasks daemon >>/run/winlux/tasks.log 2>&1 &
# Mode jeu : processeur et mémoire pour le jeu tant qu'il tourne.
/usr/bin/winlux --gamemode daemon >>/run/winlux/gamemode.log 2>&1 &
boot_mark services
# Paquets demandés par un fichier de réponse : installés une fois, dès que le
# réseau répond, mais seulement après l'arrivée du bureau et en basse priorité
# (le premier démarrage restait sinon plus d'une minute sur un écran noir).
if test -s /etc/winlux/first-boot-packages && test -x /usr/bin/apt-get; then
    /bin/busybox mkdir -p /var/log
    (
        if /bin/busybox grep -qw 'winlux.interactive=1' /proc/cmdline; then
            for attempt in $(/bin/busybox seq 1 300); do
                test -e /run/winlux/boot-recorded && break
                /bin/busybox sleep 1
            done
            /bin/busybox sleep 10
        fi
        low=
        if test -x /usr/bin/nice; then low='/usr/bin/nice -n 19'; fi
        if test -x /usr/bin/ionice; then low="$low /usr/bin/ionice -c 3"; fi
        for attempt in $(/bin/busybox seq 1 60); do
            /bin/busybox ip route | /bin/busybox grep -q '^default ' && break
            /bin/busybox sleep 5
        done
        packages=$(/bin/busybox grep -E '^[a-z0-9][a-z0-9+.-]+$' /etc/winlux/first-boot-packages | /bin/busybox tr '\n' ' ')
        # Notification sur le bureau du compte quand il n'est plus root.
        notice_as=
        if test -n "$desktop_user"; then
            notice_uid=$(/bin/busybox awk -F: -v u="$desktop_user" '$1 == u { print $3; exit }' /etc/passwd)
            notice_gid=$(/bin/busybox awk -F: -v u="$desktop_user" '$1 == u { print $4; exit }' /etc/passwd)
            notice_as="/usr/bin/setpriv --reuid=$notice_uid --regid=$notice_gid --init-groups /usr/bin/env DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$notice_uid/bus"
        fi
        $notice_as /usr/bin/winlux-first-boot-notice start $packages 2>/dev/null || true
        if $low /usr/bin/apt-get update && DEBIAN_FRONTEND=noninteractive $low /usr/bin/apt-get install -y $packages; then
            /bin/busybox mv /etc/winlux/first-boot-packages /etc/winlux/first-boot-packages.done
            $notice_as /usr/bin/winlux-first-boot-notice done 2>/dev/null || true
        else
            $notice_as /usr/bin/winlux-first-boot-notice failed 2>/dev/null || true
        fi
    ) >>/var/log/winlux-first-boot-packages.log 2>&1 &
fi
# Navigateur choisi à l'installation ou au premier démarrage : téléchargé sur
# le site officiel de son éditeur, vérifié et installé dès que le réseau
# répond, en basse priorité. Sans réseau, il est retenté au démarrage suivant
# (aucun navigateur n'est fourni dans l'image : docs/BROWSERS.md).
if test -s /etc/winlux/first-boot-browser; then
    (
        choice=$(/bin/busybox sed -n '1p' /etc/winlux/first-boot-browser)
        case "$choice" in chrome|edge|firefox) ;; *) exit 0 ;; esac
        for attempt in $(/bin/busybox seq 1 120); do
            /bin/busybox ip route | /bin/busybox grep -q '^default ' && break
            /bin/busybox sleep 5
        done
        low=
        if test -x /usr/bin/nice; then low='/usr/bin/nice -n 10'; fi
        if $low /usr/bin/winlux-browsers --default "$choice"; then
            /bin/busybox mv /etc/winlux/first-boot-browser /etc/winlux/first-boot-browser.done
        fi
    ) >>/var/log/winlux-first-boot-browser.log 2>&1 &
fi
# Navigateurs demandés hors ligne depuis Paramètres > Navigateurs : installés
# dès que le réseau répond (winlux-browsers les retire de la liste ensuite).
if test -s /etc/winlux/browser-install-pending; then
    (
        for attempt in $(/bin/busybox seq 1 120); do
            /bin/busybox ip route | /bin/busybox grep -q '^default ' && break
            /bin/busybox sleep 5
        done
        low=
        if test -x /usr/bin/nice; then low='/usr/bin/nice -n 10'; fi
        for choice in $(/bin/busybox grep -E '^(chrome|edge|firefox)$' /etc/winlux/browser-install-pending); do
            $low /usr/bin/winlux-browsers --install "$choice" || true
        done
    ) >>/var/log/winlux-first-boot-browser.log 2>&1 &
fi
# Boutiques de logiciels choisies (installateur, premier démarrage) : ajoutées
# au Store dès que le réseau répond ; sans réseau, retentées au démarrage
# suivant (docs/BOUTIQUES.md).
if test -s /etc/winlux/store-catalogs-pending && test -x /usr/bin/winlux-store-catalogs; then
    (
        for attempt in $(/bin/busybox seq 1 120); do
            /bin/busybox ip route | /bin/busybox grep -q '^default ' && break
            /bin/busybox sleep 5
        done
        low=
        if test -x /usr/bin/nice; then low='/usr/bin/nice -n 10'; fi
        $low /usr/bin/winlux-store-catalogs --resume || true
    ) >>/var/log/winlux-store-catalogs.log 2>&1 &
fi
if test -n "$browser_setup_pid" && /bin/busybox grep -qwE 'winlux\.(native|browser)-smoke=1' /proc/cmdline; then
    # Les tests de navigateurs ont besoin d'Edge déjà installé.
    wait "$browser_setup_pid" 2>/dev/null || true
fi
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
        # Pas de mise à jour automatique pendant une partie (mode jeu).
        while test -e /run/winlux/gamemode.active; do /bin/busybox sleep 30; done
        /usr/bin/winlux-update --auto-run >>/run/winlux/auto-update.log 2>&1 || true
        /usr/bin/winlux-sysupdate --tick >>/run/winlux/sysupdate-auto.log 2>&1 || true
        /bin/busybox sleep 1800
    done
) &
(
    # Historique des fichiers : chaque utilisateur selon sa fréquence.
    while :; do
        while test -e /run/winlux/gamemode.active; do /bin/busybox sleep 30; done
        /usr/bin/winlux-backup auto-run >>/run/winlux/backup-auto.log 2>&1 || true
        /bin/busybox sleep 60
    done
) &

if /bin/busybox grep -qw 'winlux.interactive=1' /proc/cmdline; then
    echo "Winlux desktop preview: starting Xorg"
    /bin/busybox touch /run/winlux/splash.stop
    # Xorg doit ouvrir sa propre session pour prendre son terminal virtuel
    # comme terminal de contrôle et gérer les changements de VT (rendre
    # l'affichage au serveur X d'un autre compte, « Changer d'utilisateur »).
    # Lancé directement par l'init (pid 1, groupe 0), il se fait chef de
    # groupe puis échoue sur setsid (« setsid failed: Operation not
    # permitted ») : le second Xorg trouvait alors l'affichage pris
    # (« drmSetMaster: Device or resource busy ») et l'écran restait noir.
    # Un petit parent chef de sa propre session lui permet de réussir.
    # Bus D-Bus système avant Xorg : sinon « (EE) dbus-core: error connecting
    # to system bus » à chaque démarrage. Quelques millisecondes ; si le compte
    # messagebus manque encore (premier démarrage), winlux-system-services le
    # lance plus tard comme avant.
    if test -x /usr/bin/dbus-daemon && ! test -S /run/dbus/system_bus_socket && /bin/busybox grep -q '^messagebus:' /etc/passwd; then
        /bin/busybox mkdir -p /run/dbus /var/lib/dbus
        /bin/busybox chmod 755 /run/dbus
        /usr/bin/dbus-uuidgen --ensure=/etc/machine-id 2>/dev/null || true
        /bin/busybox chmod 644 /etc/machine-id 2>/dev/null || true
        /usr/bin/dbus-daemon --system --fork >/run/winlux/system-bus.log 2>&1 || true
    fi
    xorg_args=':0 -background none -noreset -nolisten tcp -logfile /tmp/Xorg.0.log'
    if command -v setsid >/dev/null 2>&1; then
        # shellcheck disable=SC2016
        setsid /bin/sh -c 'trap "kill \$x 2>/dev/null" TERM INT HUP; /usr/bin/Xorg "$@" & x=$!; wait "$x"' winlux-xorg $xorg_args >/tmp/xorg-console.log 2>&1 &
    else
        # shellcheck disable=SC2086
        /usr/bin/Xorg $xorg_args >/tmp/xorg-console.log 2>&1 &
    fi
    xorg_pid=$!
    # Bus D-Bus, polkit, udisks, réseau, impression et son n'ont pas besoin de
    # l'écran : ils démarrent pendant que Xorg s'initialise.
    # Système installé : seuls les services système (root) ; le son et le bus
    # de session appartiennent au bureau du compte.
    (
        if test -d /usr/lib/locale/fr_FR.utf8; then LANG=fr_FR.UTF-8; else LANG=C.UTF-8; fi
        export LANG DISPLAY=:0 XDG_RUNTIME_DIR=/run/user/0
        if test -n "$desktop_user"; then
            if /usr/bin/winlux-system-services; then
                /bin/busybox touch /run/winlux/session-early.done
            else
                /bin/busybox touch /run/winlux/session-early.failed
            fi
            exit 0
        fi
        exec /usr/bin/winlux-native-session --early
    ) >/run/winlux/native-early.log 2>&1 &
    echo "$!" >/run/winlux/session-early.pid
    # Attendre que Xorg réponde (au plus 10 s) au lieu d'une pause fixe de 3 s.
    for attempt in $(/bin/busybox seq 1 100); do
        if test -S /tmp/.X11-unix/X0; then
            ! test -x /usr/bin/xrandr && break
            DISPLAY=:0 /usr/bin/xrandr --query >/dev/null 2>&1 && break
        fi
        /bin/busybox kill -0 "$xorg_pid" 2>/dev/null || break
        /bin/busybox sleep 0.1
    done
    boot_mark xorg
    echo "Winlux Xorg input probe:"
    /bin/busybox grep -Ei 'libinput|input device|mouse|keyboard|error|cursor|config' /tmp/Xorg.0.log 2>/dev/null || true
    if ! /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; then
        /bin/busybox cat /tmp/Xorg.0.log /tmp/xorg-console.log >&2
        /usr/bin/winlux-update --boot-failed >>/run/winlux/boot-update.log 2>&1 || true
        /bin/sh
    fi
    export DISPLAY=:0 XDG_CURRENT_DESKTOP=XFCE XDG_SESSION_DESKTOP=winlux XDG_MENU_PREFIX=xfce-
    export XDG_CONFIG_DIRS=/etc/xdg
    export XDG_DATA_DIRS="$HOME/.local/share/flatpak/exports/share:/var/lib/flatpak/exports/share:/usr/local/share:/usr/share"
    export WINLUX_NATIVE_SHELL=1
    # Proxy commun aux applis Windows et Linux (écrit par Winlux).
    if test -r /etc/winlux/proxy.env; then . /etc/winlux/proxy.env; fi
    if test -d /usr/lib/locale/fr_FR.utf8; then export LANG=fr_FR.UTF-8; else export LANG=C.UTF-8; fi
    if test -n "$desktop_user"; then
        # Services système en root, puis les deux relais root du bureau :
        # winlux-power (arrêt, redémarrage) et winlux-root (réglages système,
        # Boutique, mises à jour, volumes, partages, mots de passe).
        early_pid=$(/bin/busybox cat /run/winlux/session-early.pid 2>/dev/null || true)
        while ! test -e /run/winlux/session-early.done && ! test -e /run/winlux/session-early.failed && \
            /bin/busybox kill -0 "$early_pid" 2>/dev/null; do
            /bin/busybox sleep 0.05
        done
        if ! test -e /run/winlux/session-early.done; then
            # Lancement anticipé absent ou en échec : une seconde tentative ici.
            /bin/busybox cat /run/winlux/native-early.log >&2 2>/dev/null || true
            /usr/bin/winlux-system-services >/run/winlux/system-services.log 2>&1 || \
                /bin/busybox cat /run/winlux/system-services.log >&2
        fi
        (setsid /usr/bin/winlux-power serve >/run/winlux/power-serve.log 2>&1 &)
        (setsid /usr/bin/winlux-root serve >/run/winlux/root-relay.log 2>&1 &)
    elif export XDG_RUNTIME_DIR=/run/user/0 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/winlux/session-bus && \
        ! /usr/bin/winlux-native-session >/run/winlux/native-session.log 2>&1; then
        /bin/busybox cat /run/winlux/native-session.log >&2
        # Une version tout juste installée qui n'ouvre pas le bureau est
        # remplacée par la précédente, puis la machine redémarre.
        /usr/bin/winlux-update --boot-failed >>/run/winlux/boot-update.log 2>&1 || true
        # Sinon un écran lisible dit quoi faire, au lieu d'un écran noir.
        test -x /usr/bin/winlux-rescue-screen && /usr/bin/winlux-rescue-screen >/run/winlux/rescue-screen.log 2>&1
        /bin/sh
    fi
    boot_mark session
    # Bureau ouvert et stable pendant une minute : la mise à jour est validée.
    (
        /bin/busybox sleep 60
        if /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; then
            /usr/bin/winlux-update --confirm-boot >>/run/winlux/boot-update.log 2>&1 || true
        fi
    ) &
    # Bureau à distance : relancé au démarrage s'il était activé.
    DISPLAY=:0 /usr/bin/winlux-remote-config ensure >/dev/null 2>&1 &
    # Dossiers partagés : serveur relancé s'il y a des partages actifs.
    /usr/bin/winlux-share ensure >/dev/null 2>&1 &
    # Tâches planifiées « à l'ouverture de session ».
    /usr/bin/winlux --tasks fire logon >>/run/winlux/tasks.log 2>&1 &
    # Relevé d'état en basse priorité : il ne prend jamais le pas sur le
    # bureau ni sur les applications, au démarrage comme après.
    status_nice=
    if test -x /usr/bin/nice; then status_nice='/usr/bin/nice -n 10'; fi
    # Session du compte : son bureau publie lui-même l'état et suit les écrans.
    if test -z "$desktop_user"; then
    (
        while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
            DISPLAY=:0 WINLUX_CONTROL_PERIODIC=1 $status_nice /usr/bin/winlux-control status 2>/dev/null || true
            /bin/busybox sleep 3
        done
    ) &
    # Écrans : branchement à chaud, capot et taille de la fenêtre VM (QEMU
    # annonce un nouveau mode préféré) sont suivis par le runtime.
    (
        while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
            DISPLAY=:0 /usr/bin/winlux-display watch 2>>/run/winlux/display.log || true
            /bin/busybox sleep 1
        done
    ) &
    fi
    if /bin/busybox grep -qw 'winlux.debug=1' /proc/cmdline; then
        (
            while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
                modes=$(DISPLAY=:0 /usr/bin/xrandr --current 2>/dev/null | /bin/busybox grep -E '[*+]' | /bin/busybox tr -s ' ' | /bin/busybox tr '\n' ';')
                if test "$modes" != "${last_modes:-}"; then echo "Winlux xrandr: $modes"; last_modes=$modes; fi
                /bin/busybox sleep 1
            done
        ) &
    fi
    if test -n "$desktop_user"; then
        user_uid=$(/bin/busybox awk -F: -v u="$desktop_user" '$1 == u { print $3; exit }' /etc/passwd)
        user_gid=$(/bin/busybox awk -F: -v u="$desktop_user" '$1 == u { print $4; exit }' /etc/passwd)
        user_home=$(/bin/busybox awk -F: -v u="$desktop_user" '$1 == u { print $6; exit }' /etc/passwd)
        test -n "$user_home" || user_home=/home/$desktop_user
        /bin/busybox mkdir -p "/run/user/$user_uid" "/run/winlux/session-$desktop_user" "$user_home"
        /bin/busybox chown "$user_uid:$user_gid" "/run/user/$user_uid" "/run/winlux/session-$desktop_user"
        /bin/busybox chmod 700 "/run/user/$user_uid"
        DISPLAY=:0 /usr/bin/xhost "+si:localuser:$desktop_user" >/dev/null 2>&1 || true
        # Bureau ouvert sur :0 : arrêt et changement d'utilisateur le connaissent.
        DISPLAY=:0 /usr/bin/winlux-sessions register "$desktop_user" >/dev/null 2>&1 || true
        lock_at_start=0
        test "$desktop_autologin" = 1 || lock_at_start=1
        quick_exits=0
        session_limits=
        test -x /usr/bin/prlimit && session_limits='/usr/bin/prlimit --rtprio=20 --nice=31'
        while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
            session_started=$(/bin/busybox date +%s)
            # setsid : la session a son groupe de processus, arrêté d'un bloc.
            # prlimit : priorité temps réel (20) et nice jusqu'à -11 pour
            # PipeWire, sans RTKit (pas de systemd) : le son ne grésille plus
            # sous charge et « RTKit error » ne remplit plus les journaux.
            setsid /usr/bin/env -u DBUS_SESSION_BUS_ADDRESS -u XDG_RUNTIME_DIR -u MAIL \
                HOME="$user_home" USER="$desktop_user" LOGNAME="$desktop_user" SHELL=/bin/bash \
                XDG_RUNTIME_DIR="/run/user/$user_uid" WINLUX_SESSION_USER="$desktop_user" \
                WINLUX_PRIMARY_SESSION=1 WINLUX_LOCK_AT_START="$lock_at_start" \
                WINLUX_SESSION_LOGS="/run/winlux/session-$desktop_user" \
                $session_limits \
                /usr/bin/setpriv --reuid="$user_uid" --regid="$user_gid" --init-groups \
                /usr/bin/winlux-user-session >>"/run/winlux/session-$desktop_user.log" 2>&1 &
            session_pid=$!
            lock_at_start=0
            echo "WINLUX_DESKTOP_READY user=$desktop_user pid=$session_pid"
            if ! test -e /run/winlux/boot-recorded; then
                boot_mark desktop
                /usr/bin/winlux --boot-time record 2>/dev/null || true
                echo "WINLUX_BOOT_PHASES=$(/usr/bin/winlux --boot-time | /bin/busybox sed -n 's/^boot_phases=//p')"
            fi
            while /bin/busybox kill -0 "$session_pid" 2>/dev/null; do
                request=$(/bin/busybox cat /run/winlux/session-action 2>/dev/null)
                update=$(/bin/busybox cat /run/winlux/update-action 2>/dev/null)
                if test "$request" = restart-session && ! /usr/bin/winlux-update --busy; then
                    /bin/busybox rm -f /run/winlux/session-action
                    if test "$update" = restart-session; then /bin/busybox rm -f /run/winlux/update-action; fi
                    /bin/busybox kill -TERM "-$session_pid" 2>/dev/null || /bin/busybox kill "$session_pid" 2>/dev/null || true
                    /bin/busybox sleep 2
                    /bin/busybox kill -KILL "-$session_pid" 2>/dev/null || true
                    break
                fi
                /bin/busybox sleep 1
            done
            wait "$session_pid" 2>/dev/null || true
            # Un bureau qui ne tient pas : après trois échecs rapides, une
            # version tout juste installée est remplacée par la précédente.
            if test $(( $(/bin/busybox date +%s) - session_started )) -lt 20; then
                quick_exits=$((quick_exits + 1))
                if test "$quick_exits" -ge 3; then
                    /bin/busybox tail -n 40 "/run/winlux/session-$desktop_user.log" >&2
                    /usr/bin/winlux-update --boot-failed >>/run/winlux/boot-update.log 2>&1 || true
                    /bin/sh
                fi
            else
                quick_exits=0
            fi
            /bin/busybox sleep 1
        done
        /bin/sh
    fi
    while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
        taskbar_align=$(/bin/busybox sed -n 's/^TASKBAR_ALIGN=//p' /etc/winlux/preferences.conf 2>/dev/null)
        desktop_theme=$(/bin/busybox sed -n 's/^THEME=//p' /etc/winlux/preferences.conf 2>/dev/null)
        # Échelle du bureau : la plus grande demandée parmi les écrans
        # allumés (densité réelle ou réglage par écran), les autres écrans
        # sont réduits par RandR.
        ui_scale=$(DISPLAY=:0 /usr/bin/winlux-display session-scale 2>/dev/null)
        case "$ui_scale" in 1|1.25|1.5|1.75|2|2.5|3) ;; *) ui_scale=1 ;; esac
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
        # Bureau relancé (plantage, redémarrage de session) : picom gardait
        # alors toutes les fenêtres noires ; une remise à zéro les repeint.
        if test -n "${desktop_started:-}"; then (/bin/busybox sleep 2; /bin/busybox killall -USR1 picom 2>/dev/null) & fi
        desktop_started=1
        # Clavier et pavé tactile appliqués pendant que le bureau se dessine
        # (le pavé seul prenait plusieurs secondes avant le fond d'écran).
        keyboard_layout=$(/bin/busybox sed -n 's/^XKB_LAYOUT=//p' /etc/winlux/input.conf 2>/dev/null)
        case "$keyboard_layout" in fr|us|de|es|gb|it|pt|be) DISPLAY=:0 /usr/bin/setxkbmap "$keyboard_layout" >/dev/null 2>&1 & ;; esac
        # Pavé tactile : toucher-clic, deux doigts, défilement naturel, vitesse.
        DISPLAY=:0 /usr/bin/winlux-touchpad apply >/dev/null 2>&1 &
        echo "WINLUX_DESKTOP_READY pid=$desktop_pid"
        if ! test -e /run/winlux/boot-recorded; then
            boot_mark desktop
            /usr/bin/winlux --boot-time record 2>/dev/null || true
            echo "WINLUX_BOOT_PHASES=$(/usr/bin/winlux --boot-time | /bin/busybox sed -n 's/^boot_phases=//p')"
        fi
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
# QEMU sans accélération (TCG, 2 vCPU) : dix exécutables à la fois
# dépassent les 30 s prévus pour une machine réelle.
WINLUX_BIN=/usr/bin/winlux \
WINLUX_TESTDIR=/opt/winlux/tests \
WINLUX_TEST_TIMEOUT=120 \
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
    # Nom et logos Winlux à la place de ceux d'Ubuntu (politique de Canonical).
    sh "$root/scripts/winlux-brand-image.sh" "$stage"
    # Offre de sources GPL : liste exacte des paquets sources de l'image,
    # dans l'image et à côté de l'ISO (publiée avec chaque version).
    mkdir -p "$stage/usr/share/doc/winlux"
    python3 "$root/scripts/list-gpl-sources.py" "$stage" "$stage/usr/share/doc/winlux/sources-gpl.tsv"
    test -s "$stage/usr/share/doc/winlux/sources-gpl.tsv" || { echo 'sources-gpl.tsv vide' >&2; exit 1; }
    cp "$stage/usr/share/doc/winlux/sources-gpl.tsv" "$root/build/sources-gpl.tsv"
fi
# Aucun logiciel d'éditeur tiers non redistribuable dans l'image publiée.
sh "$root/scripts/check-image-redistributable.sh" "$stage"
# Pilotes et firmwares essentiels des PC réels dans l'image finale : la
# construction échoue si l'un d'eux disparaît (scripts/hardware-essentials.list).
python3 "$root/scripts/check-image-hardware.py" "$stage" "$kernel_release"
# Samba et wsdd ne démarrent jamais seuls : winlux-share lance smbd et wsdd
# uniquement quand l'utilisateur partage un dossier (pas de nmbd ni d'AD DC).
for unit in smbd nmbd samba samba-ad-dc wsdd; do
    rm -f "$stage/etc/winlux/services-enabled/$unit.service"
    if test -e "$stage/usr/lib/systemd/system/$unit.service" || test -e "$stage/lib/systemd/system/$unit.service"; then
        mkdir -p "$stage/etc/systemd/system"
        ln -sfn /dev/null "$stage/etc/systemd/system/$unit.service"
    fi
done

# policy-rc.d (« exit 101 ») ne sert qu'à la construction (prepare-apt-base) :
# laissé dans l'image, il empêchait tout service d'un paquet installé plus
# tard (Boutique, pilotes) de démarrer. L'installateur pose le sien le temps
# de son chroot et le retire.
if test -f "$stage/usr/sbin/policy-rc.d" && test "$(cat "$stage/usr/sbin/policy-rc.d")" = "$(printf '#!/bin/sh\nexit 101')"; then
    rm -f "$stage/usr/sbin/policy-rc.d"
fi
mkdir -p "$iso_root/boot/grub"
cp -L "$kernel" "$iso_root/boot/vmlinuz"
sh "$root/scripts/build-live-root.sh" "$stage" "$iso_root" "$kernel_release"
# Thème GRUB Winlux (logo Prisme) ; repli texte si la police manque.
sh "$root/scripts/winlux-grub-theme.sh" install "$root/assets/brand/grub" "$iso_root/boot/grub/themes/winlux"
cat >"$iso_root/boot/grub/grub.cfg" <<'GRUB'
set default=1
# UEFI 32 bits (mode mixte) : sans efi=noruntime, le noyau 64 bits plante
# dans les services UEFI sur les processeurs sans pages de 1 Go (Atom).
set winlux_efi_args=
if [ "${grub_platform}-${grub_cpu}" = "efi-i386" ]; then
    set winlux_efi_args=efi=noruntime
fi
# 5 s : le temps de lire le menu et d'appuyer sur une flèche sur un vrai PC.
set timeout=5
set timeout_style=menu
set winlux_term=console
# UEFI 32 bits : le terminal graphique de GRUB fait planter le noyau 64 bits
# au démarrage (mode mixte, vérifié sous QEMU) ; menu en texte.
if [ "${grub_platform}-${grub_cpu}" = "efi-i386" ]; then
    set winlux_term=console
elif loadfont /boot/grub/themes/winlux/unicode.pf2; then
    insmod all_video
    insmod gfxterm
    insmod png
    set gfxmode=auto
    if terminal_output gfxterm; then
        set theme=/boot/grub/themes/winlux/theme.txt
        set winlux_term=gfxterm
    fi
fi
# Terminal série seulement dans une machine virtuelle (QEMU/KVM, où les tests
# lisent et pilotent le menu par le port série). Sur un vrai PC, un port série
# sans rien au bout, ou celui que l'UEFI expose, ralentit la lecture du
# clavier : les flèches ne déplaçaient plus la sélection sur un portable.
set winlux_vendor=
smbios --type 1 --get-string 4 --set winlux_vendor
if [ "$winlux_vendor" = "QEMU" ]; then
    if serial --unit=0 --speed=115200; then
        terminal_input serial console
        terminal_output serial $winlux_term
    fi
fi
# Installation Winlux sur un disque : cherchée une fois, avant le menu. Une
# recherche ratée à l'intérieur d'une entrée fait attendre GRUB 10 s sur
# « Press any key to continue » à chaque démarrage live.
set winlux_root=
search --no-floppy --label WINLUX_ROOT --set=winlux_root
menuentry "Winlux diagnostic (headless + Xvfb)" --hotkey=d {
    linux /boot/vmlinuz console=ttyS0,115200n8 rootfstype=ramfs rdinit=/init panic=1 loglevel=4 $winlux_efi_args
    initrd /boot/winlux-initramfs.cpio.gz
}
function winlux_live {
    linux /boot/vmlinuz console=tty1 console=ttyS0,115200n8 rootfstype=ramfs rdinit=/init panic=1 quiet loglevel=3 vt.global_cursor_default=0 winlux.interactive=1 $winlux_efi_args
    initrd /boot/winlux-initramfs.cpio.gz
}
# La clé démarre toujours SON live : « Winlux Desktop » ne regarde jamais les
# disques. Avant la v0.2.18, il chargeait le noyau et la racine d'un Winlux
# déjà installé (étiquette WINLUX_ROOT) : impossible alors de réparer ou de
# réinstaller depuis la clé une machine où Winlux était installé.
menuentry "Winlux Desktop" {
    winlux_live
}
menuentry "Installer Winlux sur le disque" --hotkey=i {
    winlux_live
}
# Système déjà installé sur ce PC : entrée à part, jamais par défaut.
if test -n "$winlux_root" -a -f "($winlux_root)/boot/vmlinuz"; then
    menuentry "Démarrer le Winlux installé sur ce PC" --hotkey=h {
        if test -f "($winlux_root)/boot/winlux-split-root"; then
            linux "($winlux_root)/boot/vmlinuz" root=LABEL=WINLUX_ROOT rw rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 panic=1 quiet loglevel=3 vt.global_cursor_default=0 winlux.interactive=1 $winlux_efi_args
            initrd "($winlux_root)/boot/winlux-initramfs.cpio.gz"
        else
            linux "($winlux_root)/boot/vmlinuz" root=LABEL=WINLUX_ROOT rw rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 panic=1 quiet loglevel=3 vt.global_cursor_default=0 winlux.interactive=1 $winlux_efi_args
        fi
    }
fi
menuentry "Winlux Recovery (secours)" --hotkey=r {
    if search --no-floppy --label WINLUX_ROOT --set=winlux_root; then
        if test -f "($winlux_root)/boot/winlux-split-root"; then
            linux "($winlux_root)/boot/vmlinuz" root=LABEL=WINLUX_ROOT rw rootfstype=ext4 init=/init console=tty1 console=ttyS0,115200n8 panic=1 loglevel=3 winlux.recovery=1 $winlux_efi_args
            initrd "($winlux_root)/boot/winlux-initramfs.cpio.gz"
        else
            linux /boot/vmlinuz root=LABEL=WINLUX_ROOT rw rootfstype=ext4 console=tty1 console=ttyS0,115200n8 panic=1 loglevel=3 winlux.recovery=1 $winlux_efi_args
            initrd /boot/winlux-initramfs.cpio.gz
        fi
    else
        echo "Aucune installation Winlux (WINLUX_ROOT) trouvée sur les disques."
        echo "Le mode récupération répare un système installé."
        sleep 6
    fi
}
menuentry "Vérifier l'intégrité du média puis démarrer" --hotkey=v {
    linux /boot/vmlinuz console=tty1 console=ttyS0,115200n8 rootfstype=ramfs rdinit=/init panic=1 loglevel=3 vt.global_cursor_default=0 winlux.check-iso=1 winlux.interactive=1 $winlux_efi_args
    initrd /boot/winlux-initramfs.cpio.gz
}
menuentry "Winlux persistent network diagnostic" --hotkey=s {
    search --no-floppy --label WINLUX_ROOT --set=winlux_root
    if test -f "($winlux_root)/boot/winlux-split-root"; then
        linux "($winlux_root)/boot/vmlinuz" root=LABEL=WINLUX_ROOT rw rootfstype=ext4 init=/init console=ttyS0,115200n8 panic=1 loglevel=3 winlux.smoke=1 $winlux_efi_args
        initrd "($winlux_root)/boot/winlux-initramfs.cpio.gz"
    else
        linux "($winlux_root)/boot/vmlinuz" root=/dev/vda rw rootfstype=ext4 init=/init console=ttyS0,115200n8 panic=1 loglevel=3 winlux.smoke=1 $winlux_efi_args
    fi
}
GRUB
grub-script-check "$iso_root/boot/grub/grub.cfg"
# Garde-fou : les entrées live de la clé ne doivent jamais résoudre sur un
# disque installé (étiquette WINLUX_ROOT), sinon la clé ne sert plus à réparer.
for entry in "Winlux Desktop" "Installer Winlux sur le disque"; do
    if awk -v title="menuentry \"$entry\"" 'index($0, title) == 1 { inside = 1 }
        inside && /winlux_root|WINLUX_ROOT/ { bad = 1 }
        inside && /^}/ { inside = 0 }
        END { exit !bad }' "$iso_root/boot/grub/grub.cfg"; then
        echo "grub.cfg : l'entrée « $entry » peut démarrer un disque installé" >&2
        exit 1
    fi
done

# Answer file for unattended installs, if supplied: the installer finds it at the medium root.
if test -n "${WINLUX_AUTOINSTALL:-}"; then
    case "$WINLUX_AUTOINSTALL" in *.json) answer=winlux-autoinstall.json ;; *) answer=winlux-autoinstall.yaml ;; esac
    cp "$WINLUX_AUTOINSTALL" "$iso_root/$answer"
fi

secureboot=${WINLUX_SECUREBOOT:-0}
if test "$secureboot" = 1; then
    sh "$root/scripts/winlux-secureboot.sh" prepare-iso "$iso_root" "Winlux $(cat "$stage/usr/share/winlux/version")"
fi

# Per-file manifest checked at boot by the "Vérifier l'intégrité" entry.
(cd "$iso_root" && find . -type f ! -name winlux.sha256 ! -path './boot/grub/efi.img' -print | sed 's|^\./||' | LC_ALL=C sort | xargs -d '\n' sha256sum) >"$work/winlux.sha256"
mv "$work/winlux.sha256" "$iso_root/winlux.sha256"

mkdir -p "$(dirname -- "$out")"
if test "$secureboot" = 1; then
    sh "$root/scripts/winlux-secureboot.sh" make-iso "$iso_root" "$out"
else
    grub-mkrescue -o "$out" "$iso_root"
fi
sh "$root/scripts/check-iso-boot-paths.sh" "$out"
sha256sum "$out" >"$out.sha256"
echo "ISO live créée : $out"
echo "Somme SHA-256 : $out.sha256"
