#!/usr/bin/python3
"""Real device state for the native panel; never invent battery/network levels."""
from pathlib import Path
import html
import json
import re
import subprocess
import sys
import os
import time


# ── Langue de l'interface (Paramètres > Langue) : fr, en, es ou de ──────────

def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Alimentation secteur — aucune batterie exposée par la machine': ('Plugged in — this computer reports no battery', 'Conectado a la corriente — el equipo no informa de ninguna batería', 'Netzbetrieb — der Computer meldet keinen Akku'),
    'Audio indisponible': ('Audio unavailable', 'Audio no disponible', 'Audio nicht verfügbar'),
    'Son coupé': ('Sound muted', 'Sonido silenciado', 'Ton aus'),
    'Wi-Fi : ': ('Wi-Fi: ', 'Wi-Fi: ', 'WLAN: '),
    'Ethernet : ': ('Ethernet: ', 'Ethernet: ', 'Ethernet: '),
    'Réseau déconnecté': ('Network disconnected', 'Red desconectada', 'Netzwerk getrennt'),
    'VPN : {name}': ('VPN: {name}', 'VPN: {name}', 'VPN: {name}'),
    'Point d’accès sans fil mobile activé': ('Mobile hotspot on', 'Zona con cobertura inalámbrica móvil activada', 'Mobiler Hotspot ein'),
    'Centre de notifications': ('Notification center', 'Centro de notificaciones', 'Benachrichtigungscenter'),
    'Batterie : {level} %': ('Battery: {level}%', 'Batería: {level} %', 'Akku: {level} %'),
    'Batterie : {level} % — en charge': ('Battery: {level}% — charging', 'Batería: {level} % — cargando', 'Akku: {level} % — wird geladen'),
    'Batterie : {level} % — chargée': ('Battery: {level}% — fully charged', 'Batería: {level} % — cargada', 'Akku: {level} % — vollständig geladen'),
    'Batterie : {level} % — branchée, pas en charge': ('Battery: {level}% — plugged in, not charging', 'Batería: {level} % — conectada, sin cargar', 'Akku: {level} % — angeschlossen, wird nicht geladen'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def balance_width(screen_width, children, right_ids, left_ids=()):
    """Largeur du contrepoids placé juste après le séparateur 3 (barre centrée).

    xfce4-panel 4.20 n'a pas de zone centrale : les deux séparateurs extensibles
    se partagent l'espace libre, donc le groupe Démarrer + fenêtres se centre
    entre le bloc de gauche (L) et le bloc de droite (R), trop à gauche de R/2
    sans contrepoids. Avec un contrepoids de largeur R - L, le centre du groupe
    tombe au milieu de l'écran. children : (id du greffon, x relatif à la
    barre, largeur) ; id None pour une fenêtre sans greffon connu.

    R est la LARGEUR du bloc de droite (du premier au dernier greffon), jamais
    sa position : un contrepoids trop large repousse ce bloc vers la gauche, et
    « écran − x du bloc » se confirmait alors tout seul (3760 px en 4K, toute la
    barre collée à gauche). La marge au bord droit ne compte que si le bloc y
    est collé ; décollé, le calcul suivant le recolle puis retrouve la valeur exacte.
    """
    right = [(x, width) for plugin, x, width in children if plugin in right_ids and width > 1]
    if not right:
        return 0
    start = min(x for x, _ in right)
    end = max(x + width for x, width in right)
    margin = screen_width - end
    block = end - start + (margin if 0 <= margin <= 16 else 0)
    left = [(x, width) for plugin, x, width in children if plugin in left_ids and width > 1]
    occupied = max(x + width for x, width in left) - min(x for x, _ in left) if left else 0
    return max(0, min(block - occupied, screen_width // 2))


def _panel_children():
    """Fenêtres X des greffons de la barre du bas : (id, x relatif, largeur), largeur de la barre."""
    try:
        tree = subprocess.run(["xwininfo", "-root", "-tree"], capture_output=True, text=True, timeout=3).stdout
    except (OSError, subprocess.SubprocessError):
        return [], 0
    pattern = re.compile(r'^(\s*)(0x[0-9a-f]+) (?:"[^"]*"|\(has no name\)):?\s*(\((?:"([^"]*)")?.*?\))?\s+(\d+)x(\d+)\+-?\d+\+-?\d+\s+\+(-?\d+)\+(-?\d+)')
    panel, depth, found = None, 0, []
    for line in tree.splitlines():
        match = pattern.match(line)
        if not match:
            continue
        indent, window, _, wm_class, width, height, x, y = match.groups()
        width, height, x, y = int(width), int(height), int(x), int(y)
        if panel is None:
            if wm_class == "xfce4-panel" and width > 4 * height and width >= 640:
                panel, depth = (x, width), len(indent)
            continue
        if len(indent) <= depth:
            if wm_class == "xfce4-panel" and width > 4 * height and width >= 640:
                panel, depth = (x, width), len(indent)
                continue
            break
        found.append((window, x - panel[0], width))
    if panel is None:
        return [], 0
    panel_pid = None
    children = []
    for window, x, width in found:
        try:
            output = subprocess.run(["xprop", "-id", window, "_NET_WM_PID"], capture_output=True, text=True, timeout=2).stdout
            pid = int(output.rsplit("=", 1)[1])
            arguments = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
        except (OSError, subprocess.SubprocessError, ValueError, IndexError):
            continue
        name = Path(arguments[0].decode(errors="replace")).name
        if name.startswith("wrapper-"):
            plugin = next((int(item) for item in arguments[1:4] if item.isdigit()), None)
        elif name == "xfce4-panel":
            continue
        else:
            # Icône d'un programme dans la zone de notification (greffon interne 10).
            plugin = 10
        children.append((plugin, x, width))
    return children, panel[1]


def _seconds_since_start(pid):
    """Âge du processus, en secondes (None s'il est illisible)."""
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        uptime = float(Path("/proc/uptime").read_text().split()[0])
        return uptime - int(fields[19]) / os.sysconf("SC_CLK_TCK")
    except (OSError, ValueError, IndexError):
        return None


def _balance_cache():
    """Dernier contrepoids mesuré, gardé d'une session à l'autre (par compte)."""
    base = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache")
    return base / "winlux/taskbar-balance"


def _balance_log(message):
    """Journal de mesure du contrepoids, seulement si WINLUX_BALANCE_LOG est défini (tests)."""
    path = os.environ.get("WINLUX_BALANCE_LOG")
    if path:
        try:
            with open(path, "a", encoding="utf-8") as log:
                log.write(f"{time.monotonic():.2f} {message}\n")
        except OSError:
            pass


def _genmon_blank(command):
    """Vrai si la commande d'un genmon ne produit ni image ni texte."""
    if not command:
        return False
    try:
        output = subprocess.run(command.split(), capture_output=True, text=True, timeout=1).stdout
    except (OSError, subprocess.SubprocessError, ValueError):
        return False
    image = re.search(r"<img>\s*\S", output)
    text = re.search(r"<txt>(.*?)</txt>", output, re.S)
    return not image and not (text and text.group(1).strip())


def _settled_balance(ids, right_ids, left_ids):
    """Contrepoids, mesuré une fois le bloc de droite entièrement affiché.

    Au lancement de la barre, chaque greffon genmon (réseau, son, batterie,
    notifications, horloge…) est un processus à part qui s'insère quand il
    est prêt, d'abord dans un carré de 16 px empilé sur ses voisins. Mesuré
    trop tôt, le bloc de droite paraissait étroit : le contrepoids restait
    court jusqu'à la mise à jour suivante, et le bouton Démarrer, d'abord au
    milieu, glissait vers la gauche pendant deux secondes avant de revenir au
    centre à chaque démarrage (#602).

    Pendant les premières secondes du greffon, la réponse attend donc les
    genmon de droite (4 s au plus) : la barre ne l'affiche qu'à ce moment.
    La valeur de la session précédente, pour le même écran et le même ordre
    de greffons, sert dès qu'ils sont tous insérés ; sinon, la mesure attend
    qu'ils soient alignés sans chevauchement et que deux relevés concordent.
    Ensuite, aucune attente : un genmon resté vide ne ralentit pas les mises
    à jour suivantes.
    """
    children, screen = _panel_children()
    age = _seconds_since_start(os.getppid())
    key = f"{screen}:{','.join(map(str, ids))}"
    if age is not None and age <= 15:
        try:
            listing = subprocess.run(["xfconf-query", "-c", "xfce4-panel", "-p", "/plugins", "-l", "-v"],
                                     capture_output=True, text=True, timeout=2).stdout
        except (OSError, subprocess.SubprocessError):
            listing = ""
        genmon = {int(found[1]) for found in re.finditer(r"^/plugins/plugin-(\d+)\s+genmon\s*$", listing, re.M)}
        # Un genmon qui n'affiche rien (clavier tactile sans écran tactile,
        # greffon 12) garde une largeur de 1 px : l'attendre bloquait la
        # mesure 4 s à chaque relance et le cache n'était jamais repris.
        # Ceux-là ne comptent pas dans le bloc de droite (#887).
        commands = {int(found[1]): found[2].strip() for found in
                    re.finditer(r"^/plugins/plugin-(\d+)/command\s+(.+)$", listing, re.M)}
        expected = {plugin for plugin in (genmon & set(right_ids)) - {13}
                    if not _genmon_blank(commands.get(plugin))}
        try:
            cached = _balance_cache().read_text().split()
        except OSError:
            cached = []
        # Même écran, même ordre de greffons : la valeur gardée est la bonne,
        # rendue tout de suite. Attendre que le bloc de droite soit affiché
        # laissait le groupe centré décalé le temps de la relance de la barre (#887).
        if len(cached) == 2 and cached[0] == key and cached[1].isdigit():
            return int(cached[1])
        deadline, previous = time.monotonic() + 4, None
        while screen and expected and time.monotonic() < deadline:
            present = {plugin for plugin, _, width in children if width > 1}
            if expected <= present and len(cached) == 2 and cached[0] == key and cached[1].isdigit():
                return int(cached[1])
            block = sorted((x, width) for plugin, x, width in children if plugin in expected and width > 1)
            laid_out = all(x + width <= following for (x, width), (following, _) in zip(block, block[1:]))
            if expected <= present and laid_out and children == previous:
                break
            _balance_log(f"attente : attendus {sorted(expected)}, présents {sorted(present)}, "
                         f"alignés {laid_out}, greffons {children}")
            previous = children
            time.sleep(0.1)
            children, screen = _panel_children()
    if not screen:
        return 0
    width = balance_width(screen, children, right_ids, left_ids)
    _balance_log(f"mesure : {width} px, âge {age}, greffons {children}")
    cache = _balance_cache()
    try:
        if cache.read_text().split() != [key, str(width)]:
            raise OSError
    except OSError:
        try:
            cache.parent.mkdir(parents=True, exist_ok=True)
            cache.write_text(f"{key} {width}\n")
        except OSError:
            pass
    return width


kind = sys.argv[1]
def tray_directory():
    """Icônes de la barre : un dossier par compte. /run/winlux est collant et
    /run/winlux/tray appartenait au premier compte (0755) : un second compte
    ne pouvait y écrire ni contrepoids ni icône (PermissionError)."""
    if os.environ.get("WINLUX_TRAY_DIR"):
        candidates = [Path(os.environ["WINLUX_TRAY_DIR"])]
    elif os.getuid() == 0:
        candidates = [Path("/run/winlux/tray")]
    else:
        candidates = [Path(f"/run/winlux/tray-{os.getuid()}")]
    candidates.append(Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "winlux/tray")
    for candidate in candidates:
        try:
            candidate.mkdir(parents=True, exist_ok=True)
            if os.access(candidate, os.W_OK):
                return candidate
        except OSError:
            continue
    return candidates[-1]


directory = tray_directory()


def taskbar_preferences():
    """Réglages de la barre du compte (Personnalisation) : STYLE et SEARCH."""
    path = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "winlux/taskbar.conf"
    values = {"STYLE": "classic", "SEARCH": "box"}
    try:
        for line in path.read_text(encoding="utf-8").splitlines():
            key, _, value = line.partition("=")
            if key in values and value.strip():
                values[key] = value.strip()
    except (OSError, UnicodeDecodeError):
        pass
    return values


def taskbar_alignment():
    """Alignement choisi (TASKBAR_ALIGN), à défaut l'étirement du séparateur 3."""
    path = Path(os.environ.get("WINLUX_CONTROL_ROOT", "") + "/etc/winlux/preferences.conf")
    try:
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.startswith("TASKBAR_ALIGN=") and line[14:].strip() in ("left", "center"):
                return line[14:].strip()
    except (OSError, UnicodeDecodeError):
        pass
    try:
        expand = subprocess.run(["xfconf-query", "-c", "xfce4-panel", "-p", "/plugins/plugin-3/expand"],
                                capture_output=True, text=True, timeout=2).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        expand = ""
    return "left" if expand == "false" else "center"


def panel_dark():
    """La barre suit le thème GTK du compte (thème clair = barre claire)."""
    forced = os.environ.get("WINLUX_PANEL_THEME")
    if forced in ("dark", "light"):
        return forced == "dark"
    settings = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "gtk-3.0/settings.ini"
    name, prefer = "", ""
    try:
        for line in settings.read_text(encoding="utf-8").splitlines():
            key, _, value = line.partition("=")
            key, value = key.strip(), value.strip()
            if key == "gtk-theme-name": name = value.casefold()
            elif key == "gtk-application-prefer-dark-theme": prefer = value
    except (OSError, UnicodeDecodeError):
        return True
    if name.startswith("winlux-"):
        return "-dark" in name  # thème du compte écrit par winlux-personalize
    if "dark" in name or "inverse" in name or prefer in ("1", "true"):
        return True
    return not name  # pas encore de thème : barre sombre livrée


DARK = panel_dark()
color = "#e7ddf5" if DARK else "#2a2633"
mode = "d" if DARK else "l"


def battery_part():
    supplies = Path(os.environ.get("WINLUX_POWER_SUPPLY", "/sys/class/power_supply"))
    batteries = sorted(supplies.glob("BAT*/capacity"))
    level = None
    if batteries:
        # Une batterie absente ou en défaut expose parfois un fichier vide ou illisible.
        try:
            level = max(0, min(100, int(batteries[0].read_text().strip())))
            status = (batteries[0].parent / "status").read_text().strip()
        except (OSError, ValueError):
            level = None
    if level is None:
        body = f'<path d="M8 3v5m8-5v5M6 8h12v4a6 6 0 0 1-6 6v4m0-4a6 6 0 0 1-6-6V8" fill="none" stroke="{color}" stroke-width="1.8"/>'
        return body, _t("Alimentation secteur — aucune batterie exposée par la machine"), "power-ac", False
    body = f'<rect x="3" y="7" width="18" height="11" rx="2" fill="none" stroke="{color}" stroke-width="1.5"/><path d="M22 10v5" stroke="{color}" stroke-width="2"/><rect x="5" y="9" width="{14 * level / 100:.1f}" height="7" fill="{color}"/>'
    # Certains contrôleurs annoncent « Not charging » ou « Unknown » sur
    # secteur (seuil de charge, batterie pleine) : l'adaptateur fait foi.
    def _online(adapter):
        try:
            return ((adapter / "type").read_text().strip() in ("Mains", "USB", "USB_C", "USB_PD")
                    and (adapter / "online").read_text().strip() == "1")
        except OSError:
            return False
    plugged = status in ("Charging", "Full") or any(_online(path) for path in supplies.iterdir() if not path.name.startswith("BAT"))
    if plugged:
        outline = "#1e1b26" if DARK else "#f6f4fa"
        body += f'<path d="M13.5 5 8 13h4l-1.5 7L16 12h-4z" fill="{color}" stroke="{outline}" stroke-width="1" stroke-linejoin="round"/>'
    if status == "Charging":
        state = "Batterie : {level} % — en charge"
    elif plugged and (status == "Full" or level >= 99):
        state = "Batterie : {level} % — chargée"
    elif plugged:
        state = "Batterie : {level} % — branchée, pas en charge"
    else:
        state = "Batterie : {level} %"
    return body, _t(state).format(level=level), (f"battery-{level}-ac" if plugged else f"battery-{level}"), True


def audio_part():
    # Même détection que le panneau Son : PipeWire/PulseAudio (hors puits
    # factice « auto_null »), puis wpctl, puis ALSA.
    def run(args):
        try:
            result = subprocess.run(args, capture_output=True, text=True, timeout=2)
            return result.stdout if result.returncode == 0 else ""
        except (OSError, subprocess.SubprocessError): return ""
    level, muted = None, False
    default = run(["pactl", "get-default-sink"]).strip()
    if default and default != "auto_null":
        try:
            for sink in json.loads(run(["pactl", "-f", "json", "list", "sinks"]) or "[]"):
                if sink.get("name") == default:
                    level = max([int(str(c.get("value_percent", "0")).rstrip("%")) for c in sink.get("volume", {}).values()] or [0])
                    muted = bool(sink.get("mute"))
        except (ValueError, TypeError, AttributeError): pass
    if level is None:
        found = re.search(r"Volume:\s*([0-9.]+)", run(["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"]))
        if found: level, muted = round(float(found[1]) * 100), "MUTED" in found.string
    if level is None:
        output = run(["amixer", "get", "Master"])
        values = re.findall(r"\[(\d+)%\]", output)
        if values: level, muted = int(values[-1]), "[off]" in output
    wave = 0 if level is None or muted or level == 0 else 1 if level < 40 else 2 if level < 75 else 3
    body = f'<path d="M3 9h4l5-5v16l-5-5H3z" fill="{color}"/>'
    if not wave: body += f'<path d="m16 9 6 6m0-6-6 6" stroke="{color}" stroke-width="1.7"/>'
    for n in range(wave): body += f'<path d="M{15 + n * 3} {9 - n * 2}q{3 + n} {3 + n * 2} 0 {6 + n * 4}" fill="none" stroke="{color}" stroke-width="1.4"/>'
    tooltip = _t("Audio indisponible") if level is None else _t("Son coupé") if muted else f"Volume : {level}%"
    return body, tooltip, f"audio-{wave}"


# Profil du point d'accès sans fil mobile (winlux-hotspot, même UUID).
HOTSPOT_UUID = "6a3df0b6-9cff-4e1a-9a5e-77696e6c7578"


def tunnels():
    """(noms des VPN actifs, point d'accès actif) d'après NetworkManager (lecture permise au compte)."""
    try:
        output = subprocess.run(["nmcli", "-t", "-f", "TYPE,UUID,NAME", "connection", "show", "--active"],
                                capture_output=True, text=True, timeout=2,
                                env=dict(os.environ, LC_ALL="C")).stdout
    except (OSError, subprocess.SubprocessError):
        return [], False
    vpns, hotspot = [], False
    for line in output.splitlines():
        kind, _, rest = line.partition(":")
        ident, _, name = rest.partition(":")
        if kind in ("vpn", "wireguard"):
            vpns.append(name.replace("\\:", ":"))
        elif ident == HOTSPOT_UUID:
            hotspot = True
    return vpns, hotspot


def wifi_name(interface):
    """Nom du réseau Wi-Fi de l'interface : « Wi-Fi : wlo1 » ne disait rien (#914).

    nmcli donne le nom de la connexion active ; sans NetworkManager, iw donne
    le SSID. À défaut, le nom de l'interface reste affiché."""
    def nmcli(out):
        return out.strip()

    def iw(out):
        return next((line.split(":", 1)[1].strip() for line in out.splitlines()
                     if line.strip().startswith("SSID:")), "")

    for command, pick in ((["nmcli", "-t", "-g", "GENERAL.CONNECTION", "device", "show", interface], nmcli),
                          (["iw", "dev", interface, "link"], iw)):
        try:
            name = pick(subprocess.run(command, capture_output=True, text=True, timeout=2).stdout)
        except (OSError, subprocess.SubprocessError):
            continue
        if name and name != "--":
            return name
    return interface


def network_part():
    try:
        routes = subprocess.check_output(["ip", "-4", "route", "show", "default"], text=True, timeout=2).splitlines()
        fields = routes[0].split() if routes else []
        interface = fields[fields.index("dev") + 1] if "dev" in fields else ""
    except (OSError, subprocess.SubprocessError, IndexError): interface = ""
    active = [Path("/sys/class/net") / interface] if interface and "/" not in interface else []
    wireless = bool(active and (active[0] / "wireless").exists())
    if wireless:
        body = f'<path d="M2 9q10-10 20 0M5 13q7-7 14 0M9 17q3-3 6 0" fill="none" stroke="{color}" stroke-width="2"/><circle cx="12" cy="21" r="1.3" fill="{color}"/>'
    else:
        body = f'<rect x="3" y="4" width="18" height="12" rx="2" fill="none" stroke="{color}" stroke-width="1.6"/><path d="M12 16v5m-5 0h10" stroke="{color}" stroke-width="1.6"/>'
    if not active: body += '<path d="m4 4 16 16" stroke="#cf8ea9" stroke-width="2"/>'
    names = [wifi_name(path.name) if wireless else path.name for path in active]
    tooltip = (_t("Wi-Fi : ") if wireless else _t("Ethernet : ")) + ", ".join(names) if active else _t("Réseau déconnecté")
    # VPN actif : petit bouclier sur l'icône réseau, comme Windows 11 ; point
    # d'accès sans fil mobile : ondes en coin et une ligne de l'infobulle.
    vpns, hotspot = tunnels()
    accent = "#b69cff" if DARK else "#6a3df0"
    if vpns:
        body += (f'<path d="M17.5 12.5l4.5 1.6v3.3c0 2.8-2 4.6-4.5 5.6-2.5-1-4.5-2.8-4.5-5.6v-3.3z" fill="{accent}" '
                 f'stroke="{color}" stroke-width="0.8"/>')
        tooltip += "\n" + "\n".join(_t("VPN : {name}").format(name=name) for name in vpns)
    if hotspot:
        body += (f'<circle cx="4" cy="20" r="1.4" fill="{accent}"/><path d="M1.2 16.6q2.8-2.4 5.6 0" fill="none" '
                 f'stroke="{accent}" stroke-width="1.3"/>')
        tooltip += "\n" + _t("Point d’accès sans fil mobile activé")
    return body, tooltip, f"network-{int(bool(active))}-{int(wireless)}-{int(bool(vpns))}{int(hotspot)}"


def notification_count():
    """Notifications gardées dans le centre (pastille sur la cloche)."""
    state = Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state") / "winlux/notifications.json"
    try:
        items = json.loads(state.read_text(encoding="utf-8"))
        return len(items) if isinstance(items, list) else 0
    except (OSError, ValueError):
        return 0


def svg_file(identity, width, height, body):
    icon = directory / f"{identity}-{mode}.svg"
    if not icon.exists():
        temporary = icon.with_name(icon.name + f".{os.getpid()}")
        temporary.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">{body}</svg>')
        temporary.replace(icon)
    return icon


def screen_width():
    try:
        output = subprocess.run(["xdpyinfo"], capture_output=True, text=True, timeout=2).stdout
        found = re.search(r"dimensions:\s+(\d+)x", output)
        return int(found[1]) if found else 1920
    except (OSError, subprocess.SubprocessError):
        return 1920


# Réseau, son et batterie ouvrent les Réglages rapides, comme Windows 11 (le
# mélangeur et le panneau Alimentation s'ouvrent depuis ce panneau) ; la cloche
# ouvre les notifications.
QUICK = "/usr/bin/winlux-popup quick"

if kind == "battery":
    body, tooltip, identity, _ = battery_part()
elif kind == "audio":
    body, tooltip, identity = audio_part()
elif kind == "network":
    body, tooltip, identity = network_part()
elif kind == "status":
    # Un seul bouton pour réseau, son et batterie (#703), comme Windows 11 :
    # une cible de clic large au lieu de trois petites icônes séparées. Sans
    # batterie (PC de bureau), rien n'est affiché à sa place.
    parts = [network_part(), audio_part()]
    battery = battery_part()
    if battery[3]:
        parts.append(battery[:3])
    body = "".join(f'<g transform="translate({index * 30} 0)">{part[0]}</g>' for index, part in enumerate(parts))
    icon = svg_file("status-" + "-".join(part[2] for part in parts), 30 * len(parts) - 6, 24, body)
    tooltip = "\n".join(part[1] for part in parts)
    print(f"<img>{icon}</img><tool>{html.escape(tooltip)}</tool><click>{QUICK}</click>")
    raise SystemExit(0)
elif kind == "search":
    # Recherche de la barre (#703) : zone « Rechercher » ou simple loupe
    # (Personnalisation > Barre des tâches). Masquée, le greffon n'est plus
    # dans la barre (winlux-control align-*). Une seule image, donc un seul
    # bouton : tout le cadre est cliquable. Loupe seule sous 1500 px.
    label = {"en": "Search", "es": "Buscar", "de": "Suchen"}.get(_LANG, "Rechercher")
    accent = "#b69cff" if DARK else "#6a3df0"
    glass = (f'<circle cx="10" cy="10" r="5.6" fill="none" stroke="{color}" stroke-width="1.7"/>'
             f'<path d="M14.2 14.2 19 19" stroke="{accent}" stroke-width="2" stroke-linecap="round"/>')
    if taskbar_preferences()["SEARCH"] == "icon" or screen_width() < 1500:
        icon = svg_file("search-icon", 24, 24, glass)
    else:
        fill, stroke, text = ("#ffffff14", "#ffffff1f", "#c9c2da") if DARK else ("#ffffffd9", "#0000001a", "#5d566c")
        width = 196
        body = (f'<rect x="0.5" y="0.5" width="{width - 1}" height="31" rx="15.5" fill="{fill}" stroke="{stroke}"/>'
                f'<g transform="translate(10 4)">{glass}</g>'
                f'<text x="38" y="20.5" font-family="Inter, Noto Sans, DejaVu Sans, sans-serif" font-size="13" fill="{text}">{html.escape(label)}</text>')
        icon = svg_file(f"search-box-{_LANG}", width, 32, body)
    print(f"<img>{icon}</img><tool>{html.escape(label)} (Windows + S)</tool><click>/usr/bin/winlux-start-menu</click>")
    raise SystemExit(0)
elif kind == "balance":
    # Contrepoids de la barre centrée (greffon 13, juste après le séparateur 3).
    width = 0
    try:
        listing = subprocess.run(["xfconf-query", "-c", "xfce4-panel", "-p", "/panels/panel-1/plugin-ids"],
                                 capture_output=True, text=True, timeout=2).stdout
        # Une ligne par id ; la première annonce le nombre d'éléments, traduite
        # (« La valeur est un tableau de 12 éléments : ») : son nombre n'est pas un id.
        ids = [int(line.strip()) for line in listing.splitlines() if line.strip().isdigit()]
    except (OSError, subprocess.SubprocessError):
        ids = []
    # Barre flottante : la barre se centre d'elle-même, pas de contrepoids.
    floating = taskbar_preferences()["STYLE"] == "floating"
    # Barre à gauche : contrepoids d'un pixel, à la même place qu'au centre
    # (#920). winlux-control enregistre l'alignement avant de rafraîchir ce
    # greffon, puis seulement étire ou non le séparateur 3.
    if taskbar_alignment() == "left":
        floating = True
    if not floating and 3 in ids and 8 in ids and ids[ids.index(3) + 1:ids.index(3) + 2] == [13]:
        right = set(ids[ids.index(8) + 1:])
        width = _settled_balance(ids, right, set(ids[:ids.index(3)]))
    body = f'<rect width="{max(1, width)}" height="1" fill="none"/>'
    icon = directory / f"spacer-{width}.svg"
    if not icon.exists():
        icon.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="{max(1, width)}" height="1" viewBox="0 0 {max(1, width)} 1">{body}</svg>')
    # <tool></tool> vide : sans balise, genmon affiche son infobulle par défaut
    # (« (genmon) », commande, période) au survol de la zone à côté de Démarrer (#645).
    print(f"<img>{icon}</img><tool></tool>")
    raise SystemExit(0)
elif kind == "notifications":
    count = notification_count()
    body = f'<path d="M5 17h14l-2-4V9a5 5 0 0 0-10 0v4zm5 3h4" fill="none" stroke="{color}" stroke-width="2"/>'
    tooltip = _t("Centre de notifications")
    if count:
        # Pastille du nombre de notifications, comme Windows 11 (#703).
        badge = "9+" if count > 9 else str(count)
        accent, on_accent = ("#b69cff", "#1c1a26") if DARK else ("#6a3df0", "#ffffff")
        body = (f'<g transform="translate(0 4)">{body}</g><rect x="{11 if len(badge) == 1 else 7}" y="0" width="{13 if len(badge) == 1 else 17}" height="13" rx="6.5" fill="{accent}"/>'
                f'<text x="{17.5 if len(badge) == 1 else 15.5}" y="10" text-anchor="middle" font-family="Inter, Noto Sans, DejaVu Sans, sans-serif" font-size="9.5" font-weight="700" fill="{on_accent}">{badge}</text>')
        icon = svg_file(f"notifications-{badge}", 26, 28, body)
    else:
        icon = svg_file("notifications", 24, 24, body)
    print(f"<img>{icon}</img><tool>{html.escape(tooltip)}</tool><click>/usr/bin/winlux-popup notifications</click>")
    raise SystemExit(0)
else:
    raise SystemExit(2)
print(f"<img>{svg_file(identity, 24, 24, body)}</img><tool>{html.escape(tooltip)}</tool><click>{QUICK}</click>")
