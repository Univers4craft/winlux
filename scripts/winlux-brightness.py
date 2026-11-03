#!/usr/bin/python3
"""Luminosité de l'écran, un seul chemin pour tout Winlux.

Réglages rapides, Paramètres (Écran, Alimentation) et touches de luminosité
du clavier passent tous par ici. Du plus fiable au plus bancal :

1. rétroéclairage du noyau (/sys/class/backlight : acpi_video0, intel_backlight,
   amdgpu_bl0, nvidia_0…). Le pilote « firmware » passe avant « platform »,
   puis « raw », comme le démon d'alimentation (winlux --power), pour que
   touches et curseur règlent le même. Sans systemd ni logind, le compte n'a
   pas le droit d'écrire dans brightness : l'écriture passe par le relais
   root (`winlux-root exec winlux-brightness set N`).
2. écran de bureau en DDC/CI (ddcutil, i2c-dev) : détecté au démarrage par
   les services système (`winlux-brightness detect-ddc`, en root), réglé par
   le relais root.
3. luminosité logicielle (rampes gamma RandR, par winlux-ambiance) pour un
   écran externe ou une machine sans rien de pilotable, tant que le pilote
   graphique a des rampes gamma.
Sans aucun des trois, `get` répond method=none et l'interface cache le curseur.

  winlux-brightness get                  état (JSON) : method, percent, device
  winlux-brightness set N|+N|-N [--osd]  règle (5 à 100 %) ; --osd affiche l'indicateur
  winlux-brightness key up|down          touche du clavier (Openbox) : pas de 5 ou
                                         10 %, indicateur à l'écran
  winlux-brightness detect-ddc           (root) écrans DDC/CI → /run/winlux/brightness-ddc.json
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

SYS_ROOT = Path(os.environ.get("WINLUX_SYS_ROOT", "/"))
RUN = Path(os.environ.get("WINLUX_BRIGHTNESS_RUN", "/run/winlux"))
DDC_CACHE = RUN / "brightness-ddc.json"
POWER_CONF = SYS_ROOT / "etc/winlux/power.conf"
RELAY_SOCKET = os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")
MINIMUM = 5
SOFT_MINIMUM = 10
RANK = {"firmware": 0, "platform": 1, "raw": 2}


def _root():
    """Lancé en root (relais, session Live) ? Pour les tests, WINLUX_BRIGHTNESS_AS
    vaut « user » ou « root » (fichiers factices seulement)."""
    forced = os.environ.get("WINLUX_BRIGHTNESS_AS", "")
    return forced == "root" if forced in ("user", "root") else os.geteuid() == 0


# ── Langue ──────────────────────────────────────────────────────────────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="): code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    "Luminosité": ("Brightness", "Brillo", "Helligkeit"),
    "Luminosité de l’écran, en pourcentage": ("Screen brightness, as a percentage", "Brillo de la pantalla, en porcentaje",
                                              "Bildschirmhelligkeit in Prozent"),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# ── 1. Rétroéclairage du noyau ──────────────────────────────────────────────

def _read(path):
    try: return path.read_text().strip()
    except OSError: return ""


def _int(text, default=None):
    try: return int(text)
    except (TypeError, ValueError): return default


def backlights():
    """Rétroéclairages utilisables, le préféré en tête."""
    found = []
    for directory in sorted((SYS_ROOT / "sys/class/backlight").glob("*")):
        maximum = _int(_read(directory / "max_brightness"), 0)
        if maximum <= 0 or not (directory / "brightness").exists(): continue
        found.append((RANK.get(_read(directory / "type"), 3), directory.name, directory, maximum))
    found.sort(key=lambda item: (item[0], item[1]))
    return [(directory, maximum) for _, _, directory, maximum in found]


def raw_of(maximum, percent):
    return max(1, -(-maximum * percent // 100))


def backlight_percent(directory, maximum):
    # actual_brightness : la valeur réellement appliquée par le matériel.
    current = _int(_read(directory / "actual_brightness")) if (directory / "actual_brightness").exists() else None
    if current is None or current > maximum: current = _int(_read(directory / "brightness"), maximum)
    # Pilote à peu de crans (acpi_video0 : 15) : le pourcentage demandé en
    # dernier est gardé tant que le cran n'a pas bougé, le curseur ne saute pas.
    last = _read(RUN / "brightness-last").split()
    if len(last) == 3 and last[0] == directory.name and _int(last[1]) == current and raw_of(maximum, _int(last[2], 0)) == current:
        return _int(last[2])
    return max(0, min(100, round(current * 100 / maximum)))


def note_last(directory, maximum, percent):
    try:
        RUN.mkdir(parents=True, exist_ok=True)
        path = RUN / "brightness-last"
        path.write_text(f"{directory.name} {raw_of(maximum, percent)} {percent}\n")
        os.chmod(path, 0o666)
    except OSError:
        pass


def backlight_write(directory, maximum, percent):
    raw = raw_of(maximum, percent)
    if not _root() and not (directory / "brightness").stat().st_mode & 0o222:
        raise PermissionError(str(directory / "brightness"))
    with open(directory / "brightness", "w") as out:
        out.write(str(raw))


def remember(percent):
    """Préférence du démon d'alimentation (BRIGHTNESS de power.conf), pour qu'un
    changement de profil ou le redémarrage ne remette pas l'ancienne valeur."""
    try: text = POWER_CONF.read_text()
    except OSError: text = ""
    factor = 70 if re.search(r"^PROFILE=saver\s*$", text, re.M) else 100
    preference = max(MINIMUM, min(100, round(percent * 100 / factor)))
    lines = [line for line in text.splitlines() if not line.startswith("BRIGHTNESS=")]
    lines.append(f"BRIGHTNESS={preference}")
    try:
        POWER_CONF.parent.mkdir(parents=True, exist_ok=True)
        temporary = POWER_CONF.with_name(POWER_CONF.name + f".{os.getpid()}")
        temporary.write_text("\n".join(lines) + "\n")
        os.chmod(temporary, 0o644)
        temporary.replace(POWER_CONF)
    except OSError:
        pass


# ── 2. DDC/CI (écran de bureau) ─────────────────────────────────────────────

def ddc_displays():
    try: data = json.loads(DDC_CACHE.read_text())
    except (OSError, ValueError): return []
    return [d for d in data.get("displays", []) if isinstance(d, dict) and isinstance(d.get("bus"), int)]


def ddcutil(args, timeout=15):
    tool = shutil.which("ddcutil")
    if not tool: return None
    try:
        return subprocess.run([tool, *args], capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        return None


def ddc_read(bus):
    """(actuel, maximum) de la commande VCP 10 (luminosité), None si illisible."""
    result = ddcutil(["--bus", str(bus), "--brief", "getvcp", "10"])
    if result is None or result.returncode: return None
    match = re.search(r"VCP 10 C (\d+) (\d+)", result.stdout)
    if not match or int(match.group(2)) <= 0: return None
    return int(match.group(1)), int(match.group(2))


def detect_ddc():
    """(root) Écrans qui acceptent la luminosité en DDC/CI ; résultat en cache."""
    if os.geteuid() == 0:
        try: subprocess.run(["modprobe", "i2c-dev"], capture_output=True, timeout=10)
        except (OSError, subprocess.SubprocessError): pass
    displays = []
    result = ddcutil(["detect", "--brief"], timeout=60)
    if result is not None and result.returncode == 0:
        for bus in sorted({int(n) for n in re.findall(r"/dev/i2c-(\d+)", result.stdout)}):
            value = ddc_read(bus)
            if value: displays.append({"bus": bus, "max": value[1]})
    RUN.mkdir(parents=True, exist_ok=True)
    temporary = DDC_CACHE.with_name(DDC_CACHE.name + ".new")
    temporary.write_text(json.dumps({"displays": displays, "time": int(time.time())}))
    os.chmod(temporary, 0o644)
    temporary.replace(DDC_CACHE)
    return {"ok": True, "displays": displays}


# ── 3. Luminosité logicielle (winlux-ambiance) ──────────────────────────────

def ambiance_tool():
    if shutil.which("winlux-ambiance"): return ["winlux-ambiance"]
    script = Path(__file__).resolve().with_name("winlux-ambiance.py")
    return [sys.executable, str(script)] if script.is_file() else None


def soft_state():
    """(disponible, pourcentage) : rampes gamma de l'écran X de la session."""
    if os.environ.get("WINLUX_CONTROL_RELAYED") == "1" or not os.environ.get("DISPLAY"):
        return False, 100
    tool = ambiance_tool()
    if tool is None: return False, 100
    try:
        result = subprocess.run(tool + ["status"], capture_output=True, text=True, timeout=10)
    except (OSError, subprocess.SubprocessError):
        return False, 100
    values = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
    return values.get("supported") == "1", _int(values.get("dim"), 100)


# ── État et réglage ─────────────────────────────────────────────────────────

def get():
    lights = backlights()
    if lights:
        directory, maximum = lights[0]
        return {"ok": True, "method": "backlight", "device": directory.name,
                "percent": backlight_percent(directory, maximum)}
    displays = ddc_displays()
    if displays:
        value = ddc_read(displays[0]["bus"]) if _root() else None
        percent = round(value[0] * 100 / value[1]) if value else _int(displays[0].get("percent"), 100)
        return {"ok": True, "method": "ddc", "device": f"i2c-{displays[0]['bus']}", "percent": percent}
    supported, dim = soft_state()
    if supported:
        return {"ok": True, "method": "soft", "device": "gamma", "percent": dim}
    return {"ok": True, "method": "none", "percent": None}


def target_of(value, current):
    """« 60 », « +10 » ou « -10 » → pourcentage voulu (5 à 100)."""
    text = str(value).strip().rstrip("%")
    if not re.fullmatch(r"[+-]?\d{1,3}", text): return None
    number = int(text)
    target = (current or 0) + number if text[0] in "+-" else number
    return max(MINIMUM, min(100, target))


def relay(args):
    """Même commande, en root, par winlux-root (le compte n'écrit pas dans /sys)."""
    command = [shutil.which("winlux-root") or "/usr/bin/winlux-root", "exec", "winlux-brightness", *args]
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.SubprocessError) as error:
        return {"ok": False, "error": "relay", "detail": str(error)}
    for line in reversed(result.stdout.splitlines()):
        if line.startswith("{"):
            try: return json.loads(line)
            except ValueError: break
    return {"ok": False, "error": "relay", "detail": (result.stderr or result.stdout).strip()[-500:]}


def can_relay():
    return not _root() and os.path.exists(RELAY_SOCKET) and os.environ.get("WINLUX_CONTROL_RELAYED") != "1"


def set_value(value):
    state = get()
    method = state["method"]
    target = target_of(value, state.get("percent"))
    if target is None:
        return {"ok": False, "error": "value"}
    if method == "backlight":
        lights = backlights()
        directory, maximum = lights[0]
        try:
            backlight_write(directory, maximum, target)
            # Certains portables (Acer, MSI…) exposent deux rétroéclairages dont
            # un seul agit vraiment (acpi_video0 inerte, intel_backlight réel) :
            # tous reçoivent la même valeur, le bon finit toujours par répondre.
            for other, other_maximum in lights[1:]:
                try: backlight_write(other, other_maximum, target)
                except OSError: pass
        except PermissionError:
            if can_relay(): return relay(["set", str(target)])
            return {"ok": False, "error": "permission", "method": method}
        except OSError as error:
            return {"ok": False, "error": "failed", "detail": str(error), "method": method}
        if _root(): remember(target)
        note_last(directory, maximum, target)
        return {"ok": True, "method": method, "device": directory.name, "percent": backlight_percent(directory, maximum)}
    if method == "ddc":
        display = ddc_displays()[0]
        if not _root():
            if can_relay(): return relay(["set", str(target)])
            return {"ok": False, "error": "permission", "method": method}
        maximum = _int(display.get("max"), 100) or 100
        result = ddcutil(["--bus", str(display["bus"]), "setvcp", "10", str(round(target * maximum / 100))])
        if result is None or result.returncode:
            return {"ok": False, "error": "failed", "method": method,
                    "detail": "" if result is None else (result.stderr or result.stdout).strip()[-300:]}
        # Le compte ne lit pas l'écran en DDC/CI : la valeur est gardée pour lui.
        try:
            data = json.loads(DDC_CACHE.read_text())
            for entry in data.get("displays", []):
                if entry.get("bus") == display["bus"]: entry["percent"] = target
            DDC_CACHE.write_text(json.dumps(data))
        except (OSError, ValueError):
            pass
        return {"ok": True, "method": method, "device": state["device"], "percent": target}
    if method == "soft":
        target = max(SOFT_MINIMUM, target)
        tool = ambiance_tool()
        result = subprocess.run(tool + ["set", "DIM", str(target)], capture_output=True, text=True, timeout=15)
        if result.returncode:
            return {"ok": False, "error": "failed", "method": method, "detail": result.stderr.strip()[-300:]}
        return {"ok": True, "method": method, "device": "gamma", "percent": target}
    return {"ok": False, "error": "none", "method": method}


def step(current, up):
    """Pas des touches : 5 % sous 20 %, 10 % au-dessus (comme le démon d'alimentation)."""
    size = 5 if current < 20 or (not up and current <= 20) else 10
    return min(100, current + size) if up else max(MINIMUM, current - size)


def key(direction):
    """Touche du clavier. Le démon d'alimentation (root) traite déjà les touches
    pour le rétroéclairage du noyau : ici, on attend qu'il ait fini et on
    affiche seulement l'indicateur. Pour un écran DDC/CI ou la luminosité
    logicielle, c'est ici que se fait le pas."""
    up = direction == "up"
    state = get()
    if state["method"] == "none": return state
    before = state.get("percent")
    if state["method"] == "backlight" and os.environ.get("WINLUX_BRIGHTNESS_KEYS_HERE") != "1" and power_daemon_running():
        time.sleep(0.45)
        state = get()
        # Le démon a écrit dans un rétroéclairage inerte, ou n'a rien fait : on fait le pas.
        if state.get("percent") == before and before is not None and (before < 100 if up else before > MINIMUM):
            state = set_value(str(step(before, up)))
    else:
        state = set_value(str(step(state.get("percent") or 100, up)))
    if state.get("ok") and state.get("percent") is not None: osd(state["percent"])
    return state


def power_daemon_running():
    if os.environ.get("WINLUX_BRIGHTNESS_DAEMON") in ("0", "1"):
        return os.environ["WINLUX_BRIGHTNESS_DAEMON"] == "1"
    for status in Path("/proc").glob("[0-9]*/cmdline"):
        try: parts = status.read_bytes().split(b"\0")
        except OSError: continue
        if len(parts) > 2 and parts[0].endswith(b"winlux") and parts[1] == b"--power" and parts[2] == b"daemon":
            return True
    return False


# ── Indicateur à l'écran (comme Windows : soleil et barre) ──────────────────

def osd(percent, seconds=1.5):
    """Petit indicateur en bas de l'écran, dans un processus à part : la commande
    rend la main tout de suite. Un nouvel appui remplace l'ancien indicateur."""
    if not os.environ.get("DISPLAY") or os.environ.get("WINLUX_BRIGHTNESS_NO_OSD") == "1": return
    marker = RUN / f"brightness-osd-{os.getuid()}"
    try:
        RUN.mkdir(parents=True, exist_ok=True)
        marker.write_text(f"{percent} {time.time()}\n")
    except OSError:
        pass
    if os.fork() != 0: return
    try:
        os.setsid()
        show_osd(percent, seconds, marker)
    finally:
        os._exit(0)


def show_osd(percent, seconds, marker):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk
    window = Gtk.Window(type=Gtk.WindowType.POPUP)
    window.set_name("winlux-brightness-osd")
    window.set_title(_t("Luminosité"))
    window.set_keep_above(True)
    window.set_accept_focus(False)
    box = Gtk.Box(spacing=12)
    box.set_border_width(14)
    icon = Gtk.Image.new_from_icon_name("display-brightness-symbolic", Gtk.IconSize.LARGE_TOOLBAR)
    box.pack_start(icon, False, False, 0)
    bar = Gtk.LevelBar(min_value=0, max_value=100)
    bar.set_value(percent)
    bar.set_size_request(180, 6)
    bar.set_valign(Gtk.Align.CENTER)
    box.pack_start(bar, True, True, 0)
    label = Gtk.Label(label=f"{percent}")
    box.pack_start(label, False, False, 0)
    window.add(box)
    window.get_accessible().set_name(_t("Luminosité de l’écran, en pourcentage"))
    css = Gtk.CssProvider()
    css.load_from_data(b"#winlux-brightness-osd { background: #2b2a33; color: #f3f0fa; border-radius: 10px; }"
                       b"#winlux-brightness-osd levelbar block.filled { background: #b69cff; }")
    Gtk.StyleContext.add_provider_for_screen(window.get_screen(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
    window.show_all()
    screen = window.get_screen()
    width, height = window.get_size()
    window.move((screen.get_width() - width) // 2, screen.get_height() - height - 96)
    stamp = _read(marker)

    def done():
        Gtk.main_quit()
        return False

    def check():
        # Un appui plus récent a pris la main : on s'efface tout de suite.
        if _read(marker) != stamp: return done()
        return True
    GLib.timeout_add(int(seconds * 1000), done)
    GLib.timeout_add(100, check)
    Gtk.main()


# ── Ligne de commande ───────────────────────────────────────────────────────

def main(argv):
    args = argv[1:]
    command = args[0] if args else "get"
    if command == "get" and len(args) <= 1:
        answer = get()
    elif command == "set" and len(args) in (2, 3) and (len(args) == 2 or args[2] == "--osd"):
        answer = set_value(args[1])
        if answer.get("ok") and len(args) == 3 and answer.get("percent") is not None: osd(answer["percent"])
    elif command == "key" and len(args) == 2 and args[1] in ("up", "down"):
        answer = key(args[1])
    elif command == "detect-ddc" and len(args) == 1:
        if not _root():
            answer = {"ok": False, "error": "permission"}
        else:
            answer = detect_ddc()
    else:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    print(json.dumps(answer, ensure_ascii=False))
    return 0 if answer.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
