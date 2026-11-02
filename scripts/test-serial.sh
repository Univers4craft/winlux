#!/bin/sh
# Ports série (#271) : compile testc/msvc/serial_msvc.c avec clang (cible MSVC)
# si besoin, puis le lance sous Winlux avec COM7 = un pseudo-terminal dont
# l'autre bout (Python) renvoie en majuscules ce qu'il reçoit :
#   - QueryDosDeviceW et HKLM\HARDWARE\DEVICEMAP\SERIALCOMM trouvent COM7 ;
#   - BuildCommDCBW + SetCommState, GetCommProperties ;
#   - SetCommMask(EV_RXCHAR) + WaitCommEvent, puis la réponse ; TransmitCommChar.
# Sauté (code 0) quand le runtime, clang ou python3 manque.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
exe="$out/serial_msvc.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "python3 absent : test sauté"; exit 0; }
if ! test -f "$exe" || test "$root/testc/msvc/serial_msvc.c" -nt "$exe"; then
    sh "$root/testc/msvc/build.sh" "$out"
fi
test -f "$exe" || { echo "clang, lld-link ou llvm-dlltool absent : test sauté"; exit 0; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
set +e
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
    echo "serial_msvc.exe : le test n° $status a échoué" >&2
    exit 1
}
echo "serial_msvc : COM7 trouvé (QueryDosDevice, SERIALCOMM), réglé, échange et WaitCommEvent"
