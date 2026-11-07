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
        expected = (genmon & set(right_ids)) - {13}
        try:
            cached = _balance_cache().read_text().split()
        except OSError:
            cached = []
        deadline, previous = time.monotonic() + 4, None
        while screen and expected and time.monotonic() < deadline:
            present = {plugin for plugin, _, width in children if width > 1}
            if expected <= present and len(cached) == 2 and cached[0] == key and cached[1].isdigit():
                return int(cached[1])
            block = sorted((x, width) for plugin, x, width in children if plugin in expected and width > 1)
            laid_out = all(x + width <= following for (x, width), (following, _) in zip(block, block[1:]))
            if expected <= present and laid_out and children == previous:
                break
            previous = children
            time.sleep(0.1)
            children, screen = _panel_children()
    if not screen:
        return 0
    width = balance_width(screen, children, right_ids, left_ids)
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
color = "#e7ddf5"
if kind == "battery":
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
    if level is not None:
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
            body += f'<path d="M13.5 5 8 13h4l-1.5 7L16 12h-4z" fill="{color}" stroke="#1e1b26" stroke-width="1" stroke-linejoin="round"/>'
        if status == "Charging":
            state = "Batterie : {level} % — en charge"
        elif plugged and (status == "Full" or level >= 99):
            state = "Batterie : {level} % — chargée"
        elif plugged:
            state = "Batterie : {level} % — branchée, pas en charge"
        else:
            state = "Batterie : {level} %"
        tooltip = _t(state).format(level=level)
        identity = f"battery-{level}-ac" if plugged else f"battery-{level}"
    else:
        body = f'<path d="M8 3v5m8-5v5M6 8h12v4a6 6 0 0 1-6 6v4m0-4a6 6 0 0 1-6-6V8" fill="none" stroke="{color}" stroke-width="1.8"/>'
        tooltip, identity = _t("Alimentation secteur — aucune batterie exposée par la machine"), "power-ac"
    page = 3
elif kind == "audio":
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
    identity, page = f"audio-{wave}", 3
elif kind == "network":
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
    tooltip = (_t("Wi-Fi : ") if wireless else _t("Ethernet : ")) + ", ".join(path.name for path in active) if active else _t("Réseau déconnecté")
    identity, page = f"network-{int(bool(active))}-{int(wireless)}", 2
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
    if 3 in ids and 8 in ids and ids[ids.index(3) + 1:ids.index(3) + 2] == [13]:
        width = _settled_balance(ids, set(ids[ids.index(8) + 1:]), set(ids[:ids.index(3)]))
    body = f'<rect width="{max(1, width)}" height="1" fill="none"/>'
    icon = directory / f"spacer-{width}.svg"
    if not icon.exists():
        icon.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="{max(1, width)}" height="1" viewBox="0 0 {max(1, width)} 1">{body}</svg>')
    # <tool></tool> vide : sans balise, genmon affiche son infobulle par défaut
    # (« (genmon) », commande, période) au survol de la zone à côté de Démarrer (#645).
    print(f"<img>{icon}</img><tool></tool>")
    raise SystemExit(0)
elif kind == "notifications":
    body = f'<path d="M5 17h14l-2-4V9a5 5 0 0 0-10 0v4zm5 3h4" fill="none" stroke="{color}" stroke-width="2"/>'
    tooltip, identity, page = _t("Centre de notifications"), "notifications", 0
else:
    raise SystemExit(2)
icon = directory / (identity + ".svg")
if not icon.exists(): icon.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">{body}</svg>')
# Réseau, son et batterie ouvrent les Réglages rapides, comme Windows 11 (le
# mélangeur et le panneau Alimentation s'ouvrent depuis ce panneau) ; la cloche
# ouvre les notifications.
command = "/usr/bin/winlux-popup notifications" if kind == "notifications" else "/usr/bin/winlux-popup quick"
print(f"<img>{icon}</img><tool>{html.escape(tooltip)}</tool><click>{command}</click>")
