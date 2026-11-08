#!/usr/bin/python3
"""Réglages avancés de la souris Winlux, comme Paramètres > Bluetooth et
appareils > Souris.

    winlux-mouse status            état lisible par Paramètres (clé=valeur)
    winlux-mouse set CLÉ VALEUR    enregistre et applique un réglage
    winlux-mouse apply             réapplique tout (ouverture de session, souris branchée)
    winlux-mouse trail             traînée du pointeur (lancé par apply)

Clés : speed 0..100 (50 = normal), precision 0|1 (accélération « Améliorer la
précision du pointeur »), natural 0|1 (défilement inversé), left 0|1 (bouton
principal à droite, pour gauchers), lines 1..100 (lignes par cran de molette),
double 200..900 (ms du double-clic), color default|white|black|lime, trail
0|2..7 (longueur de la traînée).

Ce qui dépend du matériel (vitesse, précision, défilement, gaucher) n'est
annoncé que si une souris libinput l'expose : Paramètres cache le reste.
Le compte garde ses choix dans ~/.config/winlux/mouse.conf ; les programmes
Windows lisent ~/.local/share/winlux/mouse.ini (GetDoubleClickTime,
SPI_GETWHEELSCROLLLINES, SM_SWAPBUTTON, SPI_GETMOUSETRAILS…).

WINLUX_MOUSE_XINPUT remplace la commande xinput (tests).
"""
import os
from pathlib import Path
import re
import shutil
import signal
import struct
import subprocess
import sys

HOME = Path(os.environ.get("HOME") or Path.home())
CONFIG = Path(os.environ.get("XDG_CONFIG_HOME") or HOME / ".config") / "winlux/mouse.conf"
RUNTIME_INI = HOME / ".local/share/winlux/mouse.ini"
A11Y_INI = HOME / ".local/share/winlux/accessibility.ini"
ICONS = HOME / ".local/share/icons"
OPENBOX_RC = HOME / ".config/openbox/rc.xml"
SYSTEM_INPUT = Path(os.environ.get("WINLUX_CONTROL_ROOT", "/") or "/") / "etc/winlux/input.conf"
XINPUT = os.environ.get("WINLUX_MOUSE_XINPUT", "xinput")

DEFAULTS = {"speed": "50", "precision": "1", "natural": "0", "left": "0", "lines": "3",
            "double": "500", "color": "default", "trail": "0"}
COLORS = {"white": ((255, 255, 255), (0, 0, 0)), "black": ((0, 0, 0), (255, 255, 255)),
          "lime": ((160, 255, 0), (0, 0, 0))}
# Index de pointer_color du runtime PE (0 blanc, 1 noir, 2 vert vif).
A11Y_COLOR = {"default": 0, "white": 0, "black": 1, "lime": 2}


# ── Réglages ────────────────────────────────────────────────────────────────

def valid(key, value):
    value = str(value).strip()
    if key in ("precision", "natural", "left"):
        return value if value in ("0", "1") else None
    if key in ("speed", "lines", "double", "trail") and value.isdigit():
        number = int(value)
        bounds = {"speed": (0, 100), "lines": (1, 100), "double": (200, 900), "trail": (0, 7)}[key]
        if bounds[0] <= number <= bounds[1] and not (key == "trail" and number == 1):
            return str(number)
        return None
    if key == "color":
        return value if value in A11Y_COLOR else None
    return None


def load():
    values = dict(DEFAULTS)
    # Vitesse choisie avant ces réglages (Lente/Normale/Rapide du système).
    try:
        for line in SYSTEM_INPUT.read_text().splitlines():
            if line.startswith("MOUSE_SPEED=") and valid("speed", line[12:]):
                values["speed"] = line[12:].strip()
    except OSError:
        pass
    try:
        for line in CONFIG.read_text().splitlines():
            key, sep, value = line.partition("=")
            key = key.strip().lower()
            if sep and key in DEFAULTS and valid(key, value) is not None:
                values[key] = valid(key, value)
    except OSError:
        pass
    return values


def write_atomic(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(text, encoding="utf-8")
    temporary.replace(path)


def save(values):
    write_atomic(CONFIG, "# Winlux - Paramètres > Souris\n" +
                 "".join(f"{key.upper()}={values[key]}\n" for key in DEFAULTS))
    # Fichier du runtime PE : nombres seulement.
    write_atomic(RUNTIME_INI, "# Winlux - souris (programmes Windows)\n"
                 f"double_click={values['double']}\nwheel_lines={values['lines']}\n"
                 f"swap_buttons={values['left']}\ntrails={values['trail']}\n"
                 f"speed={max(1, min(20, round(int(values['speed']) / 5)))}\n")


# ── Périphériques libinput (xinput) ─────────────────────────────────────────

def xinput(*args):
    try:
        result = subprocess.run([XINPUT, *args], capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout if result.returncode == 0 else None


def props(device):
    """Propriétés libinput d'un périphérique : nom → liste de valeurs."""
    found = {}
    for line in (xinput("list-props", device) or "").splitlines():
        match = re.match(r"\s*libinput ([^(]+?) \(\d+\):\s*(.*)$", line)
        if match:
            found[match[1]] = [v.strip() for v in match[2].split(",")]
    return found


def mice():
    """Souris (et boules, trackpoints) : pointeurs libinput qui ne sont pas des
    pavés tactiles (ceux-là ont « Tapping Enabled »)."""
    devices = []
    for device in (xinput("--list", "--id-only") or "").split():
        found = props(device)
        if "Accel Speed" in found and "Tapping Enabled" not in found:
            devices.append((device, found))
    return devices


def pointers():
    """Tout pointeur libinput qui sait inverser ses boutons (gaucher)."""
    devices = []
    for device in (xinput("--list", "--id-only") or "").split():
        found = props(device)
        if "Left Handed Enabled" in found:
            devices.append((device, found))
    return devices


def apply_devices(values):
    for device, found in mice():
        xinput("set-prop", device, "libinput Accel Speed", f"{(int(values['speed']) - 50) / 50:.2f}")
        profile = found.get("Accel Profiles Available")
        if profile and "Accel Profile Enabled" in found:
            # adaptatif, plat[, personnalisé] : un seul actif.
            wanted = ["0"] * len(found["Accel Profile Enabled"])
            wanted[0 if values["precision"] == "1" else 1] = "1"
            xinput("set-prop", device, "libinput Accel Profile Enabled", *wanted)
        if "Natural Scrolling Enabled" in found:
            xinput("set-prop", device, "libinput Natural Scrolling Enabled", values["natural"])
    for device, _found in pointers():
        xinput("set-prop", device, "libinput Left Handed Enabled", values["left"])


# ── Double-clic : GTK (xsettings) et Openbox ────────────────────────────────

def xfconf_set(channel, prop, kind, value):
    if not shutil.which("xfconf-query"):
        return False
    base = ["xfconf-query", "-c", channel, "-p", prop]
    try:
        exists = subprocess.run(base, capture_output=True, timeout=3).returncode == 0
        args = base + ([] if exists else ["-n", "-t", kind]) + ["-s", str(value)]
        return subprocess.run(args, capture_output=True, timeout=3).returncode == 0
    except (OSError, subprocess.SubprocessError):
        return False


def apply_double_click(milliseconds):
    xfconf_set("xsettings", "/Net/DoubleClickTime", "int", milliseconds)
    try:
        text = OPENBOX_RC.read_text(encoding="utf-8")
    except OSError:
        return
    updated, count = re.subn(r"<doubleClickTime>\d+</doubleClickTime>",
                             f"<doubleClickTime>{milliseconds}</doubleClickTime>", text)
    if count and updated != text:
        write_atomic(OPENBOX_RC, updated)
        if shutil.which("openbox"):
            subprocess.run(["openbox", "--reconfigure"], capture_output=True, timeout=5)


# ── Couleur du pointeur : thème Xcursor recoloré ────────────────────────────

XCURSOR_IMAGE = 0xFFFD0002


def read_xcursor(data):
    """Images d'un fichier Xcursor : [(taille, largeur, hauteur, xhot, yhot, délai, pixels)]."""
    if data[:4] != b"Xcur":
        raise ValueError("pas un fichier Xcursor")
    header, _version, count = struct.unpack_from("<III", data, 4)
    images = []
    for index in range(count):
        kind, subtype, position = struct.unpack_from("<III", data, header + index * 12)
        if kind != XCURSOR_IMAGE:
            continue
        _size, _kind, _sub, _ver, width, height, xhot, yhot, delay = struct.unpack_from("<9I", data, position)
        start = position + 36
        pixels = list(struct.unpack_from(f"<{width * height}I", data, start))
        images.append((subtype, width, height, xhot, yhot, delay, pixels))
    return images


def write_xcursor(images):
    count = len(images)
    header = 16
    out = bytearray(b"Xcur" + struct.pack("<III", header, 0x10000, count))
    position = header + count * 12
    table, chunks = bytearray(), bytearray()
    for subtype, width, height, xhot, yhot, delay, pixels in images:
        table += struct.pack("<III", XCURSOR_IMAGE, subtype, position + len(chunks))
        chunks += struct.pack("<9I", 36, XCURSOR_IMAGE, subtype, 1, width, height, xhot, yhot, delay)
        chunks += struct.pack(f"<{len(pixels)}I", *pixels)
    return bytes(out + table + chunks)


def lightness(pixel):
    alpha = pixel >> 24
    if not alpha:
        return None
    r, g, b = (pixel >> 16 & 255) * 255 // alpha, (pixel >> 8 & 255) * 255 // alpha, (pixel & 255) * 255 // alpha
    return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255


def recolor(pixels, fill, outline, dark_fill):
    """Le corps du pointeur prend la couleur choisie, le contour son opposé."""
    out = []
    for pixel in pixels:
        level = lightness(pixel)
        if level is None:
            out.append(0)
            continue
        alpha = pixel >> 24
        t = level if dark_fill else 1 - level
        rgb = [round((f * (1 - t) + o * t) * alpha / 255) for f, o in zip(fill, outline)]
        out.append(alpha << 24 | rgb[0] << 16 | rgb[1] << 8 | rgb[2])
    return out


def base_theme():
    """Thème de pointeurs du système (celui que la couleur « par défaut » garde)."""
    for index in (HOME / ".icons/default/index.theme", Path("/usr/share/icons/default/index.theme")):
        try:
            match = re.search(r"^Inherits\s*=\s*([^,\n]+)", index.read_text(), re.M)
        except OSError:
            continue
        if match and not match[1].strip().startswith("Winlux-Pointeur"):
            return match[1].strip()
    return "Adwaita"


def theme_dir(name):
    for root in (ICONS, HOME / ".icons", Path("/usr/share/icons")):
        if (root / name / "cursors").is_dir():
            return root / name / "cursors"
    return None


def build_theme(color, source_name=None):
    """Recolore le thème du système dans ~/.local/share/icons/Winlux-Pointeur-COULEUR."""
    source_name = source_name or base_theme()
    source = theme_dir(source_name)
    if source is None:
        return None
    name = f"Winlux-Pointeur-{color}"
    target = ICONS / name / "cursors"
    stamp = ICONS / name / ".source"
    if target.is_dir() and stamp.is_file() and stamp.read_text().strip() == source_name:
        return name
    fill, outline = COLORS[color]
    try:
        reference = read_xcursor((source / "left_ptr").read_bytes())[0][6]
        levels = [lightness(p) for p in reference if p >> 24 > 200]
        dark_fill = sum(levels) / max(1, len(levels)) < 0.5
    except (OSError, ValueError, IndexError, struct.error):
        dark_fill = True
    if target.exists():
        shutil.rmtree(target)
    target.mkdir(parents=True)
    for entry in sorted(source.iterdir()):
        destination = target / entry.name
        if entry.is_symlink():
            destination.symlink_to(os.readlink(entry))
            continue
        try:
            images = read_xcursor(entry.read_bytes())
            images = [(s, w, h, x, y, d, recolor(p, fill, outline, dark_fill)) for s, w, h, x, y, d, p in images]
            destination.write_bytes(write_xcursor(images))
        except (OSError, ValueError, struct.error):
            continue
    (ICONS / name / "index.theme").write_text(f"[Icon Theme]\nName={name}\nInherits={source_name}\n")
    stamp.write_text(source_name + "\n")
    return name


def apply_color(color):
    theme = base_theme() if color == "default" else build_theme(color)
    if theme is None:
        return False
    xfconf_set("xsettings", "/Gtk/CursorThemeName", "string", theme)
    # Programmes X sans xsettings (Openbox, fenêtres X11) au prochain démarrage.
    write_atomic(HOME / ".icons/default/index.theme",
                 f"[Icon Theme]\nInherits={theme if color != 'default' else base_theme()}\n")
    # Pointeur dessiné par les programmes Windows (runtime PE).
    a11y_set("pointer_color", A11Y_COLOR[color])
    return True


def a11y_set(key, value):
    """Une clé de accessibility.ini, sans toucher aux autres (fichier en Windows-1252)."""
    try:
        lines = A11Y_INI.read_text(encoding="latin-1").splitlines()
    except OSError:
        lines = ["# Winlux - Param\xe8tres > Accessibilit\xe9"]
    for index, line in enumerate(lines):
        if line.partition("=")[0].strip() == key:
            lines[index] = f"{key}={value}"
            break
    else:
        lines.append(f"{key}={value}")
    A11Y_INI.parent.mkdir(parents=True, exist_ok=True)
    temporary = A11Y_INI.with_name(A11Y_INI.name + ".tmp")
    temporary.write_text("\n".join(lines) + "\n", encoding="latin-1", errors="replace")
    temporary.replace(A11Y_INI)


# ── Traînée du pointeur ─────────────────────────────────────────────────────

def trail_pidfile():
    return Path(os.environ.get("XDG_RUNTIME_DIR") or f"/tmp/winlux-{os.getuid()}") / "winlux-mouse-trail.pid"


def stop_trail():
    try:
        pid = int(trail_pidfile().read_text())
        os.kill(pid, signal.SIGTERM)
    except (OSError, ValueError):
        pass
    try:
        trail_pidfile().unlink()
    except OSError:
        pass


def start_trail():
    stop_trail()
    if not os.environ.get("DISPLAY"):
        return
    command = ["/usr/bin/winlux-mouse"] if Path("/usr/bin/winlux-mouse").exists() else [sys.executable, str(Path(__file__).resolve())]
    process = subprocess.Popen(command + ["trail"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               start_new_session=True)
    trail_pidfile().parent.mkdir(parents=True, exist_ok=True)
    trail_pidfile().write_text(f"{process.pid}\n")


ARROW = ((0, 0), (0, 17), (4, 13), (7, 20), (10, 19), (7, 12), (12, 12))


def run_trail():
    """Copies du pointeur qui suivent la souris quand elle bouge, comme
    l'option « Afficher les traces du pointeur » de Windows."""
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    import cairo

    values = load()
    length = int(values["trail"])
    if length < 2:
        return 0
    fill, outline = COLORS.get(values["color"], COLORS["white"])
    size = 24
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, size, size)
    cr = cairo.Context(surface)
    cr.translate(1.5, 1.5)
    cr.move_to(*ARROW[0])
    for point in ARROW[1:]:
        cr.line_to(*point)
    cr.close_path()
    cr.set_source_rgb(*(c / 255 for c in fill))
    cr.fill_preserve()
    cr.set_line_width(1.5)
    cr.set_source_rgb(*(c / 255 for c in outline))
    cr.stroke()
    shape = Gdk.cairo_region_create_from_surface(surface)
    windows = []
    for _ in range(length):
        window = Gtk.Window(type=Gtk.WindowType.POPUP)
        window.set_default_size(size, size)
        window.set_app_paintable(True)
        window.connect("draw", lambda _w, context: (context.set_source_surface(surface, 0, 0), context.paint()) and False)
        window.realize()
        window.shape_combine_region(shape)
        window.input_shape_combine_region(cairo.Region())
        windows.append(window)
    seat = Gdk.Display.get_default().get_default_seat()
    history = []

    def tick():
        _screen, x, y = seat.get_pointer().get_position()
        if history and history[-1] == (x, y):
            # Souris immobile : la traînée se résorbe.
            history.pop(0)
        else:
            history.append((x, y))
        del history[:-(length * 2 + 1)]
        points = history[:-1][::-1][1::2]
        for index, window in enumerate(windows):
            if index < len(points):
                window.move(*points[index])
                if not window.get_visible():
                    window.show()
            elif window.get_visible():
                window.hide()
        return True

    GLib.timeout_add(16, tick)
    signal.signal(signal.SIGTERM, lambda *_: Gtk.main_quit())
    Gtk.main()
    return 0


# ── Commandes ───────────────────────────────────────────────────────────────

def status():
    values = load()
    devices = mice()
    lines = [f"{key}={value}" for key, value in values.items()]
    lines.append(f"mouse_present={int(bool(devices))}")
    lines.append(f"cap_speed={int(bool(devices))}")
    lines.append(f"cap_precision={int(any('Accel Profile Enabled' in f and 'Accel Profiles Available' in f for _d, f in devices))}")
    lines.append(f"cap_natural={int(any('Natural Scrolling Enabled' in f for _d, f in devices))}")
    lines.append(f"cap_left={int(bool(pointers()))}")
    lines.append(f"cap_color={int(theme_dir(base_theme()) is not None)}")
    return "\n".join(lines)


def apply(values=None, keys=None):
    values = values or load()
    keys = keys or set(DEFAULTS)
    if keys & {"speed", "precision", "natural", "left"}:
        apply_devices(values)
    if "double" in keys:
        apply_double_click(values["double"])
    if "color" in keys and (values["color"] != "default" or keys != set(DEFAULTS)):
        apply_color(values["color"])
    if keys & {"trail", "color"}:
        if values["trail"] != "0":
            start_trail()
        elif keys != set(DEFAULTS) or trail_pidfile().exists():
            stop_trail()


def main(argv):
    command = argv[1] if len(argv) > 1 else "status"
    if command == "status":
        print(status())
        return 0
    if command == "set" and len(argv) == 4:
        key = argv[2].lower()
        value = valid(key, argv[3]) if key in DEFAULTS else None
        if value is None:
            print(f"Valeur refusée : {argv[2]}={argv[3]}", file=sys.stderr)
            return 2
        values = load()
        values[key] = value
        save(values)
        apply(values, {key})
        return 0
    if command == "apply":
        values = load()
        if CONFIG.exists():
            save(values)
        apply(values)
        return 0
    if command == "trail":
        return run_trail()
    print(__doc__.strip().split("\n\n")[1], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
