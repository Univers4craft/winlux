#!/bin/sh
# Logiciels d'éclairage et de périphériques (#1523) : compile
# testc/msvc/hid_rgb_msvc.c avec clang (cible MSVC) si besoin, puis le lance
# sous Winlux face à un faux clavier RVB branché en USB (sysfs factice avec
# l'arbre appareil/interface, FIFO à la place de /dev/hidraw0) :
#   - deux collections de tête (clavier, interface fabricant 0xFF00) donnent
#     deux interfaces &mi_02&col01 / &col02, comme sous Windows ;
#   - cfgmgr32 rend identifiants d'instance et identifiants matériels ;
#   - un appareil fermé (hidraw1 sans nœud) s'ouvre sans droit d'accès pour
#     ses attributs seulement ;
#   - chaînes USB, capacités, 120 trames de couleurs à plus de 60 par
#     seconde, rapports d'entrée filtrés par collection.
# Sauté (code 0) quand le runtime ou clang manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/hid_rgb_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/hid_rgb_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
usb="$work/sys/devices/usb1/1-1"
intf="$usb/1-1:1.2"
hid="$intf/0003:1209:0001.0001"
mkdir -p "$hid" "$work/sys/class/hidraw/hidraw0" "$work/sys/class/hidraw/hidraw1/device" "$work/dev"
ln -s "$hid" "$work/sys/class/hidraw/hidraw0/device"
printf 'Winlux Essais\n' >"$usb/manufacturer"
printf 'Clavier RVB\n' >"$usb/product"
printf 'RVB0001\n' >"$usb/serial"
printf '0105\n' >"$usb/bcdDevice"
printf '02\n' >"$intf/bInterfaceNumber"
# Descripteur d'appareil USB : iManufacturer 1, iProduct 2, iSerialNumber 3.
printf '\022\001\000\002\000\000\000\100\011\022\001\000\005\001\001\002\003\001' >"$usb/descriptors"
printf 'DRIVER=hid-generic\nHID_ID=0003:00001209:00000001\nHID_NAME=Winlux Essais Clavier RVB\nHID_UNIQ=\n' >"$hid/uevent"
# Clavier (rapport 1 : 8 modificateurs) puis collection fabricant 0xFF00 :
# Feature 7 de 64 octets, Output 8 de 32 octets.
printf '\005\001\011\006\241\001\205\001\005\007\031\340\051\347\025\000\045\001\165\001\225\010\201\002\300\006\000\377\011\001\241\001\205\007\011\002\025\000\046\377\000\165\010\226\100\000\261\002\205\010\011\003\225\040\221\002\300' >"$hid/report_descriptor"
mkfifo "$work/dev/hidraw0"
# Second appareil sans nœud /dev : fermé à l'utilisateur.
printf 'DRIVER=hid-generic\nHID_ID=0003:00001209:00000002\nHID_NAME=Winlux Souris fermee\n' >"$work/sys/class/hidraw/hidraw1/device/uevent"
printf '\005\001\011\002\241\001\011\001\241\000\005\011\031\001\051\003\025\000\045\001\225\003\165\001\201\002\225\001\165\005\201\001\300\300' >"$work/sys/class/hidraw/hidraw1/device/report_descriptor"

cd "$work"
set +e
WINLUX_GUI=headless WINLUX_HIDRAW_SYSFS="$work/sys/class/hidraw" WINLUX_HIDRAW_DEV="$work/dev" \
    WINLUX_SYSFS="$work/sys" timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    cat "$work/out.txt"
    echo "hid_rgb_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "hid_rgb_msvc : collections de tête, attributs sans droit, chaînes USB, 120 trames de couleurs, touches décodées"
