#!/usr/bin/env python3
"""Rotation automatique de l'écran des tablettes et des convertibles.

    winlux-rotation watch      suit l'accéléromètre (lancé par la session)
    winlux-rotation status     orientation lue, écran, verrou
    winlux-rotation lock       garde l'orientation actuelle
    winlux-rotation unlock     l'écran suit de nouveau la tablette

Lit l'accéléromètre IIO du noyau (hid-sensor-accel-3d, bmc150, kxcjk-1013,
mxc4005…, embarqués depuis #401) avec sa matrice de montage (propriété udev
ACCEL_MOUNT_MATRIX de 60-sensor.hwdb, sinon in_accel_mount_matrix), puis
tourne l'écran interne avec xrandr et recale l'écran tactile et le stylet
(xinput map-to-output), comme Windows. Convention d'iio-sensor-proxy : écran
droit en paysage, la gravité lue vaut y < 0.
Sur un convertible (châssis 31), l'écran ne tourne qu'en mode tablette
(interrupteur SW_TABLET_MODE). Sans accéléromètre, « watch » s'arrête aussitôt.
Verrou : ~/.config/winlux/rotation.conf (LOCK=1).

Variables de test : WINLUX_HW_ROOT, WINLUX_ROTATION_ONCE=1 (une lecture).
"""
import array
import fcntl
import glob
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(os.environ.get("WINLUX_HW_ROOT", "/"))
CONF = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "winlux/rotation.conf"
XRANDR = {"normal": "normal", "bottom-up": "inverted", "left-up": "left", "right-up": "right"}
TOUCH = re.compile(r"touch ?screen|touch$|pen|stylus|wacom|digitizer|goodix|silead|elan.*(touch|pen)|ntrg|gxtp|wcom", re.I)
NOT_TOUCH = re.compile(r"touchpad|trackpad|clickpad|keyboard|mouse", re.I)


def read(path, default=""):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return default


def run(args):
    try:
        return subprocess.run(args, capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def accelerometer():
    for device in sorted(glob.glob(str(ROOT / "sys/bus/iio/devices/iio:device*"))):
        if all(os.path.exists(f"{device}/in_accel_{axis}_raw") for axis in "xyz"):
            return Path(device)
    return None


def parse_matrix(text):
    """« 1, 0, 0; 0, 1, 0; 0, 0, 1 » → 3×3 ; None si illisible."""
    try:
        rows = [[float(v) for v in row.split(",")] for row in text.strip().split(";")]
    except ValueError:
        return None
    return rows if len(rows) == 3 and all(len(r) == 3 for r in rows) else None


def mount_matrix(device):
    udev = run(["udevadm", "info", "--query=property", f"--path={device}"]) if str(ROOT) == "/" else ""
    match = re.search(r"^ACCEL_MOUNT_MATRIX=(.*)$", udev, re.M)
    for text in ((match.group(1) if match else ""), read(device / "in_accel_mount_matrix"), read(device / "in_mount_matrix")):
        matrix = parse_matrix(text) if text else None
        if matrix:
            return matrix
    return [[1, 0, 0], [0, 1, 0], [0, 0, 1]]


def reading(device, matrix):
    raw = [float(read(device / f"in_accel_{axis}_raw", "0") or 0) for axis in "xyz"]
    return [sum(matrix[i][j] * raw[j] for j in range(3)) for i in range(3)]


def orientation(x, y, z, current="normal"):
    """Orientation de l'écran d'après la gravité ; à plat ou ambigu : inchangée."""
    total = (x * x + y * y + z * z) ** 0.5
    if total == 0 or (x * x + y * y) ** 0.5 < 0.45 * total:
        return current  # posée à plat
    if abs(y) > 1.4 * abs(x):
        return "normal" if y < 0 else "bottom-up"
    if abs(x) > 1.4 * abs(y):
        return "left-up" if x > 0 else "right-up"
    return current  # en diagonale : on garde l'orientation


def chassis():
    return read(ROOT / "sys/class/dmi/id/chassis_type")


EV_SW, SW_TABLET_MODE = 0x05, 0x01


def tablet_mode():
    """None : aucun interrupteur mode tablette ; sinon True/False."""
    found = None
    for device in sorted(glob.glob(str(ROOT / "sys/class/input/event*"))):
        caps = read(f"{device}/device/capabilities/sw", "0").split()
        try:
            bits = int(caps[-1], 16) if caps else 0
        except ValueError:
            bits = 0
        if not bits & (1 << SW_TABLET_MODE):
            continue
        found = False
        try:
            with open(f"/dev/input/{os.path.basename(device)}", "rb") as handle:
                state = array.array("B", [0] * 8)
                # EVIOCGSW(len) = _IOC(READ, 'E', 0x1b, len)
                fcntl.ioctl(handle, (2 << 30) | (len(state) << 16) | (ord("E") << 8) | 0x1B, state)
                if state[0] & (1 << SW_TABLET_MODE):
                    return True
        except OSError:
            pass
    return found


def internal_output():
    text = run(["xrandr", "--query"])
    connected = [line.split()[0] for line in text.splitlines() if " connected" in line]
    for name in connected:
        if re.match(r"(eDP|LVDS|DSI)", name):
            return name
    return connected[0] if connected else ""


def current_rotation(output):
    for line in run(["xrandr", "--query"]).splitlines():
        if line.startswith(output + " connected"):
            head = line.split("(", 1)[0]
            for word in ("inverted", "left", "right"):
                if f" {word}" in head:
                    return word
            return "normal"
    return "normal"


def touch_devices():
    names = []
    for line in run(["xinput", "list", "--name-only"]).splitlines():
        name = line.strip()
        if name and TOUCH.search(name) and not NOT_TOUCH.search(name):
            names.append(name)
    return names


def apply(state):
    output = internal_output()
    if not output:
        return False
    subprocess.run(["xrandr", "--output", output, "--rotate", XRANDR[state]], check=False)
    for name in touch_devices():
        subprocess.run(["xinput", "map-to-output", name, output], check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return True


def locked():
    return re.search(r"^LOCK=1\s*$", read(CONF), re.M) is not None


def set_lock(value):
    CONF.parent.mkdir(parents=True, exist_ok=True)
    CONF.write_text(f"LOCK={1 if value else 0}\n")


def watch():
    device = accelerometer()
    if device is None:
        print("Aucun accéléromètre : rotation automatique inutile ici.")
        return 0
    if chassis() not in ("30", "31", "32") and os.environ.get("WINLUX_ROTATION_ANY") != "1":
        print("Ni tablette ni convertible : l'écran ne tourne pas tout seul.")
        return 0
    matrix = mount_matrix(device)
    convertible = chassis() == "31"
    names = {v: k for k, v in XRANDR.items()}
    shown = names.get(current_rotation(internal_output()), "normal")
    candidate, streak = shown, 0
    while True:
        allowed = not locked() and (not convertible or tablet_mode() is not False)
        state = orientation(*reading(device, matrix), current=shown)
        if allowed and state != shown:
            streak = streak + 1 if state == candidate else 1
            candidate = state
            # Deux lectures concordantes (1 s) : pas de bascule en passant.
            if streak >= 2 or os.environ.get("WINLUX_ROTATION_ONCE") == "1":
                if apply(state):
                    shown = state
                streak = 0
        else:
            streak = 0
        if os.environ.get("WINLUX_ROTATION_ONCE") == "1":
            return 0
        time.sleep(0.5)


def status():
    device = accelerometer()
    print(f"accelerometer={read(device / 'name') if device else ''}")
    if device:
        print("orientation=" + orientation(*reading(device, mount_matrix(device))))
    print(f"chassis={chassis()}")
    print(f"tablet_mode={tablet_mode()}")
    print(f"locked={int(locked())}")
    return 0


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else "status"
    if command == "watch":
        return watch()
    if command == "status":
        return status()
    if command in ("lock", "unlock"):
        set_lock(command == "lock")
        return 0
    print(__doc__.strip(), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
