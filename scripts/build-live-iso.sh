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
fonts=${WINLUX_X11_FONTS:-/usr/share/fonts/X11/misc}
xkb=${WINLUX_XKB_DATA:-/usr/share/X11/xkb}

for tool in cargo bash cpio fakeroot find gzip grub-mkrescue ldd sha256sum xorriso; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "outil requis absent : $tool" >&2
        exit 1
    }
done
for path in "$kernel" /usr/bin/busybox /usr/bin/Xvfb /usr/bin/xkbcomp "$fonts" "$xkb"; do
    if [ "$path" = "$kernel" ]; then continue; fi
    test -e "$path" || { echo "fichier requis absent : $path" >&2; exit 1; }
done

mkdir -p "$root/build"
rm -rf "$work"
mkdir -p \
    "$stage/bin" "$stage/dev" "$stage/etc" "$stage/lib" "$stage/lib64" \
    "$stage/opt/winlux/scripts" "$stage/opt/winlux/tests" "$stage/proc" \
    "$stage/root/.local/share/winlux" "$stage/run" "$stage/sys" "$stage/tmp" \
    "$stage/usr/bin" "$stage/usr/lib" "$stage/usr/share/fonts/X11" "$stage/usr/share/X11"

cd "$root"
cargo build --release -p winlux
bash scripts/build-tests.sh

cp "$root/target/release/winlux" "$stage/usr/bin/winlux"
cp "$root/scripts/test-runtime.sh" "$stage/opt/winlux/scripts/test-runtime.sh"
chmod 755 "$stage/usr/bin/winlux" "$stage/opt/winlux/scripts/test-runtime.sh"
for name in hello exitcode p1_suite dll_import delay_import findfiles crt_suite gui_suite gui_demo; do
    cp "$root/target/x86_64-pc-windows-msvc/release/$name.exe" "$stage/opt/winlux/tests/$name.exe"
done

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
ln -s ../../bin/busybox "$stage/usr/bin/dirname"
cp /usr/bin/Xvfb "$stage/usr/bin/Xvfb"
cp /usr/bin/xkbcomp "$stage/usr/bin/xkbcomp"
chmod 755 "$stage/usr/bin/Xvfb" "$stage/usr/bin/xkbcomp"
copy_shared_libraries "$root/target/release/winlux"
copy_shared_libraries /usr/bin/Xvfb
copy_shared_libraries /usr/bin/xkbcomp
cp -a "$fonts" "$stage/usr/share/fonts/X11/misc"
cp -a "$xkb" "$stage/usr/share/X11/xkb"

cat >"$stage/init" <<'INIT'
#!/bin/sh
PATH=/bin:/usr/bin:/sbin:/usr/sbin
HOME=/root
XKB_CONFIG_ROOT=/usr/share/X11/xkb
export PATH HOME XKB_CONFIG_ROOT

/bin/busybox mount -t proc proc /proc 2>/dev/null || true
/bin/busybox mount -t sysfs sysfs /sys 2>/dev/null || true
/bin/busybox mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
/bin/busybox mkdir -p /tmp/.X11-unix /run /root/.local/share/winlux
/bin/busybox chmod 1777 /tmp

echo "Winlux live test: headless PE suite"
WINLUX_BIN=/usr/bin/winlux \
WINLUX_TESTDIR=/opt/winlux/tests \
    /bin/sh /opt/winlux/scripts/test-runtime.sh
headless_status=$?
echo "WINLUX_LIVE_HEADLESS_EXIT=$headless_status"

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
if [ "$headless_status" -ne 0 ] || [ "$x11_status" -ne 0 ]; then
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
GRUB

mkdir -p "$(dirname -- "$out")"
grub-mkrescue -o "$out" "$iso_root"
sha256sum "$out" >"$out.sha256"
echo "ISO live créée : $out"
echo "Somme SHA-256 : $out.sha256"
