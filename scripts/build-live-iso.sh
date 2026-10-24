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

for tool in cargo bash cpio fakeroot find gzip grub-mkrescue ldd ldconfig sha256sum xorriso zstd; do
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
for path in /usr/bin/busybox /usr/bin/curl /usr/bin/install /usr/bin/Xvfb /usr/bin/xkbcomp "$xorg" "$xorg_modules" "$fonts" "$xkb" "$truetype_fonts" /etc/fonts /etc/ssl/certs/ca-certificates.crt; do
    test -e "$path" || { echo "fichier requis absent : $path" >&2; exit 1; }
done
test -r /usr/share/X11/xorg.conf.d/40-libinput.conf || { echo "configuration libinput Xorg absente" >&2; exit 1; }
test -r "$xft_library" || { echo "bibliothèque Xft absente : $xft_library" >&2; exit 1; }
for module in "$virtio_dma_module" "$virtio_gpu_module" "$virtio_input_module" "$serio_raw_module" "$psmouse_module" "$hid_module" "$usbhid_module" "$hid_generic_module"; do
    test -r "$module" || { echo "module noyau requis absent : $module" >&2; exit 1; }
done

mkdir -p "$root/build"
rm -rf "$work"
mkdir -p \
    "$stage/bin" "$stage/dev" "$stage/etc" "$stage/lib" "$stage/lib64" \
    "$stage/lib/modules/$kernel_release" \
    "$stage/opt/winlux/scripts" "$stage/opt/winlux/tests" "$stage/proc" \
    "$stage/root/.local/share/winlux" "$stage/run" "$stage/sys" "$stage/tmp" \
    "$stage/usr/bin" "$stage/usr/lib" "$stage/usr/lib/xorg" "$stage/usr/share/fonts/X11" \
    "$stage/usr/share/applications" "$stage/usr/share/fonts/truetype" \
    "$stage/usr/share/icons" "$stage/usr/share/pixmaps" \
    "$stage/usr/share/X11" "$stage/etc/fonts"

cd "$root"
cargo build --release -p winlux
bash scripts/build-tests.sh

cp "$root/target/release/winlux" "$stage/usr/bin/winlux"
cp "$root/scripts/winlux-update.sh" "$stage/usr/bin/winlux-update"
mkdir -p "$stage/usr/share/winlux"
git -C "$root" describe --tags --always >"$stage/usr/share/winlux/version"
cp "$root/scripts/test-runtime.sh" "$stage/opt/winlux/scripts/test-runtime.sh"
cp "$root/scripts/test-runtime-parallel.sh" "$stage/opt/winlux/scripts/test-runtime-parallel.sh"
chmod 755 \
    "$stage/usr/bin/winlux" "$stage/usr/bin/winlux-update" \
    "$stage/opt/winlux/scripts/test-runtime.sh" \
    "$stage/opt/winlux/scripts/test-runtime-parallel.sh"
for name in hello exitcode p1_suite dll_import delay_import findfiles crt_suite gui_suite gui_demo; do
    cp "$root/target/x86_64-pc-windows-msvc/release/$name.exe" "$stage/opt/winlux/tests/$name.exe"
done
cp "$root/target/x86_64-pc-windows-msvc/release/testdll.dll" "$stage/opt/winlux/tests/testdll.dll"

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
for applet in cp cut grep head mkdir mv rm sed sha256sum tar tr wc; do
    ln -s ../../bin/busybox "$stage/usr/bin/$applet"
done
cp /usr/bin/curl "$stage/usr/bin/curl"
cp /usr/bin/install "$stage/usr/bin/install"
cp /usr/bin/Xvfb "$stage/usr/bin/Xvfb"
cp /usr/bin/xkbcomp "$stage/usr/bin/xkbcomp"
cp "$xorg" "$stage/usr/bin/Xorg"
zstd -q -d -c "$virtio_dma_module" >"$stage/lib/modules/$kernel_release/virtio_dma_buf.ko"
zstd -q -d -c "$virtio_gpu_module" >"$stage/lib/modules/$kernel_release/virtio-gpu.ko"
zstd -q -d -c "$virtio_input_module" >"$stage/lib/modules/$kernel_release/virtio-input.ko"
zstd -q -d -c "$serio_raw_module" >"$stage/lib/modules/$kernel_release/serio-raw.ko"
zstd -q -d -c "$psmouse_module" >"$stage/lib/modules/$kernel_release/psmouse.ko"
zstd -q -d -c "$hid_module" >"$stage/lib/modules/$kernel_release/hid.ko"
zstd -q -d -c "$usbhid_module" >"$stage/lib/modules/$kernel_release/usbhid.ko"
zstd -q -d -c "$hid_generic_module" >"$stage/lib/modules/$kernel_release/hid-generic.ko"
chmod 755 "$stage/usr/bin/Xvfb" "$stage/usr/bin/xkbcomp" "$stage/usr/bin/Xorg"
copy_shared_libraries "$root/target/release/winlux"
copy_shared_libraries /usr/bin/Xvfb
copy_shared_libraries /usr/bin/xkbcomp
copy_shared_libraries /usr/bin/curl
copy_shared_libraries /usr/bin/install
copy_shared_libraries "$xorg"
mkdir -p "$stage$(dirname -- "$xft_library")"
cp -L "$xft_library" "$stage$xft_library"
copy_shared_libraries "$xft_library"
cp -a "$xorg_modules" "$stage/usr/lib/xorg/modules"
find "$xorg_modules" -type f -name '*.so' | while IFS= read -r module; do
    copy_shared_libraries "$module"
done
cp -a "$fonts" "$stage/usr/share/fonts/X11/misc"
cp -a "$xkb" "$stage/usr/share/X11/xkb"
mkdir -p "$stage/usr/share/X11/xorg.conf.d"
cp /usr/share/X11/xorg.conf.d/40-libinput.conf "$stage/usr/share/X11/xorg.conf.d/40-libinput.conf"
cp -a "$truetype_fonts" "$stage/usr/share/fonts/truetype/dejavu"
cp -a /etc/fonts/. "$stage/etc/fonts/"
mkdir -p "$stage/etc/ssl/certs"
cp /etc/ssl/certs/ca-certificates.crt "$stage/etc/ssl/certs/ca-certificates.crt"
if test -d /usr/share/fontconfig; then
    cp -a /usr/share/fontconfig "$stage/usr/share/fontconfig"
fi

# Le shell ne copie que les icônes réellement utilisées dans la préversion VM.
for icon in preferences-desktop distributor-logo-linux-mint; do
    source=$(find -L /usr/share/icons /usr/share/pixmaps -type f -name "$icon.png" 2>/dev/null | \
        awk 'NR == 1 { fallback=$0 } /\/256(@2x)?\// { print; found=1; exit } /\/128(@2x)?\// { candidate=$0 } END { if (!found) print candidate ? candidate : fallback }' | head -1)
    if test -n "$source"; then
        cp "$source" "$stage/usr/share/pixmaps/$icon.png"
    fi
done
cat >"$stage/init" <<'INIT'
#!/bin/sh
PATH=/bin:/usr/bin:/sbin:/usr/sbin
HOME=/root
XKB_CONFIG_ROOT=/usr/share/X11/xkb
export PATH HOME XKB_CONFIG_ROOT

/bin/busybox mount -t proc proc /proc 2>/dev/null || true
/bin/busybox mount -t sysfs sysfs /sys 2>/dev/null || true
/bin/busybox mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/virtio_dma_buf.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/virtio-gpu.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/virtio-input.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/hid.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/usbhid.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/hid-generic.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/serio-raw.ko 2>/dev/null || true
/bin/busybox insmod /lib/modules/$(/bin/busybox uname -r)/psmouse.ko 2>/dev/null || true
/bin/busybox mdev -s 2>/dev/null || true
/bin/busybox mdev -s 2>/dev/null || true
/bin/busybox mkdir -p /tmp/.X11-unix /run /root/.local/share/winlux
/bin/busybox chmod 1777 /tmp
/bin/busybox rm -f /tmp/.X0-lock /tmp/.X11-unix/X0
/bin/busybox ip link set lo up

if /bin/busybox grep -qw 'winlux.interactive=1' /proc/cmdline; then
    echo "Winlux desktop preview: starting Xorg"
    /usr/bin/Xorg :0 -noreset -nolisten tcp -logfile /tmp/Xorg.0.log >/tmp/xorg-console.log 2>&1 &
    xorg_pid=$!
    /bin/busybox sleep 3
    if ! /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; then
        /bin/busybox cat /tmp/Xorg.0.log /tmp/xorg-console.log >&2
        /bin/sh
    fi
    while /bin/busybox kill -0 "$xorg_pid" 2>/dev/null; do
        tray_battery=0
        tray_battery_present=0
        for capacity in /sys/class/power_supply/BAT*/capacity; do
            if test -r "$capacity"; then tray_battery=$(/bin/busybox cat "$capacity"); tray_battery_present=1; break; fi
        done
        tray_network=0
        for state in /sys/class/net/*/operstate; do
            if test -r "$state" && test "$(/bin/busybox cat "$state")" = up; then tray_network=1; break; fi
        done
        DISPLAY=:0 WINLUX_DESKTOP_DEMO=1 WINLUX_UI_SCALE=1.5 WINLUX_TRAY_VOLUME=70 WINLUX_TRAY_BATTERY="$tray_battery" WINLUX_TRAY_BATTERY_PRESENT="$tray_battery_present" WINLUX_TRAY_NETWORK="$tray_network" \
            /usr/bin/winlux /opt/winlux/tests/gui_demo.exe
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
/usr/bin/Xvfb :99 -screen 0 1920x1080x24 -nolisten tcp \
    -fp /usr/share/fonts/X11/misc,built-ins >/tmp/xvfb.log 2>&1 &
xvfb_pid=$!
/bin/busybox sleep 1
x11_status=1
if /bin/busybox kill -0 "$xvfb_pid" 2>/dev/null; then
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
   [ "$x11_status" -ne 0 ]; then
    result=1
fi
echo "WINLUX_LIVE_RESULT=$result"
/bin/busybox sync
/bin/busybox poweroff -f
while :; do /bin/busybox sleep 2; done
INIT
chmod 755 "$stage/init"

export WINLUX_LIVE_STAGE="$stage"
export WINLUX_LIVE_CPIO="$work/winlux-initramfs.cpio"
fakeroot sh -c '
    set -eu
    stage=$WINLUX_LIVE_STAGE
    mknod -m 600 "$stage/dev/console" c 5 1
    mknod -m 666 "$stage/dev/null" c 1 3
    mknod -m 666 "$stage/dev/zero" c 1 5
    mknod -m 666 "$stage/dev/tty" c 5 0
    mknod -m 666 "$stage/dev/ttyS0" c 4 64
    cd "$stage"
    find . -print0 | cpio --null --create --format=newc --quiet
' >"$WINLUX_LIVE_CPIO"
gzip -n -9 -c "$WINLUX_LIVE_CPIO" >"$work/winlux-initramfs.cpio.gz"

mkdir -p "$iso_root/boot/grub"
cp -L "$kernel" "$iso_root/boot/vmlinuz"
cp "$work/winlux-initramfs.cpio.gz" "$iso_root/boot/winlux-initramfs.cpio.gz"
cat >"$iso_root/boot/grub/grub.cfg" <<'GRUB'
set default=0
set timeout=3
if serial --unit=0 --speed=115200; then
    terminal_input serial console
    terminal_output serial console
fi
menuentry "Winlux live test (headless + Xvfb)" {
    linux /boot/vmlinuz console=ttyS0,115200n8 rdinit=/init panic=1 loglevel=4
    initrd /boot/winlux-initramfs.cpio.gz
}
menuentry "Winlux desktop preview (1920x1080)" {
    linux /boot/vmlinuz console=tty1 console=ttyS0,115200n8 rdinit=/init panic=1 loglevel=3 winlux.interactive=1 video=1920x1080
    initrd /boot/winlux-initramfs.cpio.gz
}
GRUB

mkdir -p "$(dirname -- "$out")"
grub-mkrescue -o "$out" "$iso_root"
sha256sum "$out" >"$out.sha256"
echo "ISO live créée : $out"
echo "Somme SHA-256 : $out.sha256"
