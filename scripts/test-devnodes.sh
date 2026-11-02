#!/bin/sh
# Arbre des périphériques (#271, partie 4) : compile testc/msvc/devnodes_msvc.c
# avec clang (cible MSVC) si besoin, puis le lance sous Winlux avec :
#   - un faux sysfs : ttyACM0 d'une « Arduino Uno » (VID 2341, PID 0043,
#     série SN123, interface 00), qui devient COM5 ;
#   - COM7 = un pseudo-terminal dont l'autre bout (Python) renvoie en
#     majuscules ce qu'il reçoit.
# Le programme cherche les ports comme pyserial (classe Ports, PortName,
# FRIENDLYNAME, HARDWAREID, parent USB par cfgmgr32), parcourt l'arbre,
# ouvre COM7 par son chemin d'interface COMPORT et lit le format de l'écran
# par GetDIBits.
# Sauté (code 0) quand le runtime, clang ou python3 manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/devnodes_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "python3 absent : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/devnodes_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
usb="$work/sys/devices/usb1/1-1"
mkdir -p "$usb/1-1:1.0" "$work/sys/class/tty/ttyACM0" "$work/dev" "$work/hidraw" "$work/home"
printf '2341\n' >"$usb/idVendor"
printf '0043\n' >"$usb/idProduct"
printf 'SN123\n' >"$usb/serial"
printf 'Arduino Uno\n' >"$usb/product"
printf 'Arduino\n' >"$usb/manufacturer"
printf '00\n' >"$usb/1-1:1.0/bInterfaceNumber"
ln -s "$usb/1-1:1.0" "$work/sys/class/tty/ttyACM0/device"
: >"$work/dev/ttyACM0"

cd "$work"
set +e
HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config" XDG_DATA_HOME="$work/home/.local/share" \
WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
WINLUX_SYSFS="$work/sys" WINLUX_DEV_ROOT="$work/dev" WINLUX_HIDRAW_SYSFS="$work/hidraw" \
python3 -B - "$runtime" "$exe" >"$work/out.txt" 2>&1 <<'PY'
import os, pty, select, subprocess, sys, time
master, slave = pty.openpty()
name = os.ttyname(slave)
env = dict(os.environ, WINLUX_GUI="headless", WINLUX_COM7=name)
proc = subprocess.Popen([sys.argv[1], sys.argv[2]], env=env)
end = time.time() + 60
while proc.poll() is None and time.time() < end:
    r, _, _ = select.select([master], [], [], 0.1)
    if r:
        try:
            data = os.read(master, 256)
        except OSError:
            break
        os.write(master, data.upper())
if proc.poll() is None:
    proc.kill()
    sys.exit(124)
sys.exit(proc.returncode)
PY
status=$?
set -e
test "$status" -eq 0 || {
    cat "$work/out.txt"
    echo "devnodes_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "devnodes_msvc : ports trouvés comme pyserial (PortName, parent USB), arbre parcouru, COM7 ouvert par son interface, écran en 32 bits"
