#!/bin/sh
# setupapi en x86 (#1656) : testc/wow32/setupapi32.c, construit ici avec
# clang (i686-pc-windows-msvc), énumère un faux port série et vérifie que
# Winlux remplit les structures à leur taille 32 bits (pack(1), ULONG_PTR de
# 4 octets) sans écrire au-delà. Sauté (code 0) sans runtime ni clang.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/setupapi32.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for d in clang lld-link llvm-dlltool; do
    command -v "$d" >/dev/null 2>&1 || { echo "$d absent : test sauté"; exit 0; }
done
if ! test -f "$exe" || test "$root/testc/wow32/setupapi32.c" -nt "$exe" || test "$0" -nt "$exe"; then
    tmp="$out/wow32-setupapi"
    mkdir -p "$tmp"
    printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess@4\nGetLastError@0\n' >"$tmp/kernel32.def"
    printf 'LIBRARY setupapi.dll\nEXPORTS\nSetupDiGetClassDevsW@16\nSetupDiEnumDeviceInfo@12\nSetupDiEnumDeviceInterfaces@20\nSetupDiGetDeviceInterfaceDetailW@24\nSetupDiGetDeviceInstanceIdW@20\nSetupDiDestroyDeviceInfoList@4\nSetupDiGetDeviceInstallParamsW@12\nSetupOpenInfFileW@16\nSetupFindFirstLineW@16\nSetupGetStringFieldW@20\nSetupCloseInfFile@4\n' >"$tmp/setupapi.def"
    for d in kernel32 setupapi; do
        llvm-dlltool -m i386 -k -d "$tmp/$d.def" -l "$tmp/$d.lib"
    done
    clang --target=i686-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$root/testc/wow32/setupapi32.c" -o "$tmp/setupapi32.obj"
    lld-link /nologo /entry:entry /subsystem:console /nodefaultlib /safeseh:no "$tmp/setupapi32.obj" \
        "$tmp/kernel32.lib" "$tmp/setupapi.lib" /out:"$exe"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
usb="$work/sys/devices/usb1/1-1"
mkdir -p "$usb/1-1:1.0" "$work/sys/class/tty/ttyACM0" "$work/dev" "$work/hidraw" "$work/home"
printf '2341\n' >"$usb/idVendor"
printf '0043\n' >"$usb/idProduct"
printf 'Arduino Uno\n' >"$usb/product"
printf '00\n' >"$usb/1-1:1.0/bInterfaceNumber"
ln -s "$usb/1-1:1.0" "$work/sys/class/tty/ttyACM0/device"
: >"$work/dev/ttyACM0"
printf '[Version]\r\nSignature="$WINDOWS NT$"\r\nClass=Ports\r\n' >"$work/essai32.inf"

cd "$work"
set +e
env WINLUX_GUI=headless HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
    WINLUX_SYSFS="$work/sys" WINLUX_DEV_ROOT="$work/dev" WINLUX_HIDRAW_SYSFS="$work/hidraw" \
    WINLUX_REGISTRY_PATH="$work/registre.tsv" TMPDIR="$work" \
    timeout 60 "$runtime" "$exe" >"$work/out.txt" 2>&1
status=$?
set -e
test "$status" -eq 0 || {
    tail -20 "$work/out.txt"
    echo "setupapi32.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "setupapi32 : SP_DEVINFO_DATA et SP_DEVICE_INTERFACE_DATA de 28 octets, INFCONTEXT de 16, SP_DEVINSTALL_PARAMS_W de 556, aucune écriture au-delà"
