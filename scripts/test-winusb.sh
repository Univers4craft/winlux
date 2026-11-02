#!/bin/sh
# winusb (#271, partie 5) : compile testc/msvc/winusb_msvc.c avec clang (cible
# MSVC) si besoin, puis le lance sous Winlux avec un faux sysfs : appareil USB
# 16D0:1234 (série WUSB01, « Carte de test », 480 Mbit/s), une interface à deux
# points bulk de 512 octets, sans usbfs. Le programme le trouve par setupapi
# (GUID_DEVINTERFACE_USB_DEVICE), l'ouvre, lit vitesse, descripteurs,
# interface et points par WinUSB, et vérifie le refus propre des transferts.
# Sauté (code 0) quand le runtime, clang ou python3 manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/winusb_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "python3 absent : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/winusb_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
dev="$work/sys/devices/pci0000:00/usb1/1-3"
mkdir -p "$dev" "$work/sys/bus/usb/devices" "$work/dev" "$work/hidraw" "$work/home"
ln -s "$dev" "$work/sys/bus/usb/devices/1-3"
printf '16d0\n' >"$dev/idVendor"
printf '1234\n' >"$dev/idProduct"
printf 'WUSB01\n' >"$dev/serial"
printf 'Carte de test\n' >"$dev/product"
printf 'Atelier\n' >"$dev/manufacturer"
printf '480\n' >"$dev/speed"
printf '1\n' >"$dev/busnum"
printf '5\n' >"$dev/devnum"
python3 -I -c '
import sys
d = bytes([18,1,0,2,0xFF,0,0,64,0xD0,0x16,0x34,0x12,0,1,1,2,3,1,
           9,2,32,0,1,1,0,0x80,50, 9,4,0,0,2,0xFF,0,0,0,
           7,5,0x81,2,0,2,0, 7,5,0x02,2,0,2,0])
open(sys.argv[1], "wb").write(d)' "$dev/descriptors"

cd "$work"
set +e
HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config" XDG_DATA_HOME="$work/home/.local/share" \
WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" WINLUX_GUI=headless \
WINLUX_SYSFS="$work/sys" WINLUX_DEV_ROOT="$work/dev" WINLUX_HIDRAW_SYSFS="$work/hidraw" \
    timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    cat "$work/out.txt"
    echo "winusb_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "winusb_msvc : appareil USB trouvé par setupapi, ouvert, décrit par WinUSB (vitesse, descripteurs, points), transferts refusés sans usbfs"
