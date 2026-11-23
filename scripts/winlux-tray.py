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
    '1 nouvelle notification': ('1 new notification', '1 notificación nueva', '1 neue Benachrichtigung'),
    '{count} nouvelles notifications': ('{count} new notifications', '{count} notificaciones nuevas', '{count} neue Benachrichtigungen'),
    'Batterie : {level} %': ('Battery: {level}%', 'Batería: {level} %', 'Akku: {level} %'),
    'Batterie : {level} % — en charge': ('Battery: {level}% — charging', 'Batería: {level} % — cargando', 'Akku: {level} % — wird geladen'),
    'Batterie : {level} % — chargée': ('Battery: {level}% — fully charged', 'Batería: {level} % — cargada', 'Akku: {level} % — vollständig geladen'),
    'Batterie : {level} % — branchée, pas en charge': ('Battery: {level}% — plugged in, not charging', 'Batería: {level} % — conectada, sin cargar', 'Akku: {level} % — angeschlossen, wird nicht geladen'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def balance_sides(ids):
    """(greffons du bloc de droite, greffons à gauche du séparateur 3) de la
    barre centrée, ou None sans contrepoids 13 juste après le séparateur 3.

    Le contrepoids n'est jamais compté dans le bloc de gauche, même listé
    aussi avant le séparateur 3 (#2467) : il se serait retranché de lui-même."""
    if 3 not in ids or 8 not in ids or ids[ids.index(3) + 1:ids.index(3) + 2] != [13]:
        return None
    return set(ids[ids.index(8) + 1:]), set(ids[:ids.index(3)]) - {13}


# Repère du bloc de droite (#2467) : genmon d'un pixel posé juste après le
# séparateur 8. Le bloc commence souvent par un greffon interne à la barre
# (bouton de l'assistant, flèche des icônes cachées), sans fenêtre X : seul
# ce repère, externe, donne son vrai bord gauche.
def balance_width(screen_width, children, right_ids, left_ids=(), edge=46):
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

    Avec le repère `edge` (greffon 46, juste après le séparateur 8), le bloc
    va de ce repère au bout de la barre : les greffons internes du début du
    bloc (bouton de l'assistant, flèche des icônes cachées), sans fenêtre X,
    comptent alors aussi. Sans lui, ils manquaient : contrepoids trop court de
    leur largeur et groupe décalé de la moitié vers la gauche (#2467).
    """
    marks = [x for plugin, x, _ in children if edge is not None and plugin == edge and plugin in right_ids]
    right = [(x, width) for plugin, x, width in children if plugin in right_ids and width > 1]
    if marks and 0 <= min(marks) < screen_width:
        block = screen_width - min(marks)
    elif right:
        start = min(x for x, _ in right)
        end = max(x + width for x, width in right)
        margin = screen_width - end
        block = end - start + (margin if 0 <= margin <= 16 else 0)
    else:
        return 0
    # Le contrepoids n'est jamais dans le bloc de gauche (#2467).
    left = [(x, width) for plugin, x, width in children if plugin in left_ids and plugin != 13 and width > 1]
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
                log.write(f"{time.monotonic():.2f} [{os.getpid()}] {message}\n")
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


def _settled_balance(ids, right_ids, left_ids, parent=None):
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
    age = _seconds_since_start(parent or os.getppid())
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
        # Le repère 46 ne fait qu'un pixel : l'attendre bloquerait la mesure.
        expected = {plugin for plugin in (genmon & set(right_ids)) - {13, 46}
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
    # Barre en train de se réarranger (bascule gauche ↔ centre, fenêtre qui
    # s'ouvre) : une mesure isolée pouvait servir un contrepoids faux pendant
    # 2 s, et le groupe sautait de plusieurs centaines de pixels (#2467). La
    # valeur n'est retenue que si une seconde mesure la confirme ; sinon la
    # précédente reste.
    for _ in range(5):
        time.sleep(0.15)
        again, again_screen = _panel_children()
        if not again_screen:
            break
        second = balance_width(again_screen, again, right_ids, left_ids)
        if second == width:
            break
        _balance_log(f"instable : {width} puis {second} px")
        width, children = second, again
    else:
        try:
            cached = _balance_cache().read_text().split()
        except OSError:
            cached = []
        _balance_log("instable : valeur précédente gardée")
        return int(cached[1]) if len(cached) == 2 and cached[0] == key and cached[1].isdigit() else (width or 0)
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


# Icônes cachées derrière la flèche de la zone de notification : le greffon
# systray d'XFCE retient chaque icône vue (known-legacy-items pour XEmbed,
# known-items pour StatusNotifier) et masque celles de hidden-*. Écrit en
# direct dans xfconf, appliqué sans relancer la barre.
#   winlux-tray overflow list            → JSON [{name, hidden, kind}]
#   winlux-tray overflow hide|show NOM
OVERFLOW_KINDS = (("legacy", "known-legacy-items", "hidden-legacy-items"), ("sni", "known-items", "hidden-items"))
# Toujours cachées, jamais proposées : nm-applet double l'icône réseau de
# Winlux ; winlux-notify-icon.py était le nom commun des icônes .exe d'avant.
OVERFLOW_SKIP = {"nm-applet", "winlux-notify-icon.py", "winlux-notify-icon"}


def _overflow_values(prop):
    try:
        result = subprocess.run(["xfconf-query", "-c", "xfce4-panel", "-p", f"/plugins/plugin-10/{prop}"],
                                capture_output=True, text=True, timeout=3)
    except (OSError, subprocess.SubprocessError):
        return []
    if result.returncode:
        return []
    lines = result.stdout.splitlines()
    # « Value is an array with N items: » puis une ligne vide, traduits.
    return [line.strip() for line in lines[2:] if line.strip()] if lines and ":" in lines[0] else [line.strip() for line in lines if line.strip()]


def overflow_list():
    items = []
    for kind_name, known, hidden in OVERFLOW_KINDS:
        hidden_names = set(_overflow_values(hidden))
        for name in _overflow_values(known):
            if name not in OVERFLOW_SKIP and all(item["name"] != name for item in items):
                items.append({"name": name, "hidden": name in hidden_names, "kind": kind_name})
    return items


def overflow_set(name, hide):
    changed = False
    for kind_name, known, hidden in OVERFLOW_KINDS:
        if name not in _overflow_values(known):
            continue
        values = [value for value in _overflow_values(hidden) if value != name] + ([name] if hide else [])
        args = ["xfconf-query", "-c", "xfce4-panel", "-p", f"/plugins/plugin-10/{hidden}"]
        if values:
            command = args + ["-n", "-a"] + [part for value in values for part in ("-t", "string", "-s", value)]
            if subprocess.run(command, capture_output=True, timeout=3).returncode:
                subprocess.run(args + ["-a"] + [part for value in values for part in ("-t", "string", "-s", value)],
                               capture_output=True, timeout=3)
        else:
            subprocess.run(args + ["-r"], capture_output=True, timeout=3)
        changed = True
    return changed


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


def refresh_settings():
    """Langue et thème relus : le service résident suit leurs changements."""
    global _LANG, DARK, color, mode
    _LANG = _interface_lang()
    DARK = panel_dark()
    color = "#e7ddf5" if DARK else "#2a2633"
    mode = "d" if DARK else "l"


refresh_settings()


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
    """Notifications nouvelles, pas encore vues (pastille sur la cloche, #3057).

    Comme Windows 11, la pastille compte ce qui est arrivé depuis la dernière
    ouverture du centre ; les notifications déjà vues restent dans le centre
    sans la rallumer. Le démon tient ce compte dans un petit fichier : la
    barre ne relit plus tout l'historique toutes les deux secondes."""
    state = Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state") / "winlux/notifications-nouvelles"
    try:
        value = state.read_text(encoding="ascii").strip()
        return int(value) if value.isdigit() else 0
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

_OSK = []


def _osk_module():
    """Module du clavier tactile, chargé une seule fois (script voisin, sinon
    /usr/bin/winlux-osk) ; None s'il manque ou ne se charge pas."""
    if not _OSK:
        module = None
        for path in (Path(__file__).resolve().with_name("winlux-osk.py"), Path("/usr/bin/winlux-osk")):
            if path.is_file():
                try:
                    import importlib.machinery
                    import importlib.util
                    loader = importlib.machinery.SourceFileLoader("winlux_osk_tray", str(path))
                    spec = importlib.util.spec_from_loader("winlux_osk_tray", loader)
                    module = importlib.util.module_from_spec(spec)
                    loader.exec_module(module)
                except Exception:
                    module = None
                break
        _OSK.append(module)
    return _OSK[0]


def render(kind, parent=None):
    """Ligne genmon d'un greffon (<img>, <tool>, <click>) ; None si inconnu."""
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
        return f"<img>{icon}</img><tool>{html.escape(tooltip)}</tool><click>{QUICK}</click>"
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
        return f"<img>{icon}</img><tool>{html.escape(label)} (Windows + S)</tool><click>/usr/bin/winlux-start-menu</click>"
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
        sides = None if floating else balance_sides(ids)
        if sides:
            width = _settled_balance(ids, *sides, parent)
        body = f'<rect width="{max(1, width)}" height="1" fill="none"/>'
        icon = directory / f"spacer-{width}.svg"
        if not icon.exists():
            # Écrit à part puis renommé : le service et un calcul direct
            # peuvent créer le même fichier en même temps, et genmon ne doit
            # jamais lire une image à moitié écrite (contrepoids ignoré).
            temporary = icon.with_name(icon.name + f".{os.getpid()}")
            temporary.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="{max(1, width)}" height="1" viewBox="0 0 {max(1, width)} 1">{body}</svg>')
            temporary.replace(icon)
        # <tool></tool> vide : sans balise, genmon affiche son infobulle par défaut
        # (« (genmon) », commande, période) au survol de la zone à côté de Démarrer (#645).
        return f"<img>{icon}</img><tool></tool>"
    elif kind == "edge":
        # Repère du bloc de droite (#2467) : un pixel transparent, sans infobulle.
        icon = directory / "spacer-1.svg"
        if not icon.exists():
            temporary = icon.with_name(icon.name + f".{os.getpid()}")
            temporary.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1" viewBox="0 0 1 1">'
                                 '<rect width="1" height="1" fill="none"/></svg>')
            temporary.replace(icon)
        return f"<img>{icon}</img><tool></tool>"
    elif kind == "osk":
        # Icône du clavier tactile (greffon 12) : calculée ici, dans le service
        # déjà lancé, et non plus par un Python neuf toutes les 5 s (#3419).
        module = _osk_module()
        if module is None:
            return "<txt></txt><tool></tool>"
        module._LANG = _LANG  # langue relue par refresh_settings()
        return module.tray_output()
    elif kind == "notifications":
        count = notification_count()
        body = f'<path d="M5 17h14l-2-4V9a5 5 0 0 0-10 0v4zm5 3h4" fill="none" stroke="{color}" stroke-width="2"/>'
        tooltip = _t("Centre de notifications")
        if count:
            tooltip += "\n" + (_t("1 nouvelle notification") if count == 1
                               else _t("{count} nouvelles notifications").format(count=count))
            # Pastille du nombre de notifications, comme Windows 11 (#703).
            badge = "9+" if count > 9 else str(count)
            accent, on_accent = ("#b69cff", "#1c1a26") if DARK else ("#6a3df0", "#ffffff")
            body = (f'<g transform="translate(0 4)">{body}</g><rect x="{11 if len(badge) == 1 else 7}" y="0" width="{13 if len(badge) == 1 else 17}" height="13" rx="6.5" fill="{accent}"/>'
                    f'<text x="{17.5 if len(badge) == 1 else 15.5}" y="10" text-anchor="middle" font-family="Inter, Noto Sans, DejaVu Sans, sans-serif" font-size="9.5" font-weight="700" fill="{on_accent}">{badge}</text>')
            icon = svg_file(f"notifications-{badge}", 26, 28, body)
        else:
            icon = svg_file("notifications", 24, 24, body)
        return f"<img>{icon}</img><tool>{html.escape(tooltip)}</tool><click>/usr/bin/winlux-popup notifications</click>"
    else:
        return None
    return f"<img>{svg_file(identity, 24, 24, body)}</img><tool>{html.escape(tooltip)}</tool><click>{QUICK}</click>"


# ── Service résident (winlux-tray serve) ─────────────────────────────────────
# Chaque greffon genmon lançait un Python toutes les 2 à 5 s (environ 80 par
# minute pour la barre). Le service calcule les mêmes lignes dans un seul
# processus et les dépose dans OUTPUT ; /usr/bin/winlux-tray (lecteur en sh,
# scripts/winlux-tray-read.sh) les recopie sans démarrer Python, et retombe
# sur un calcul direct quand le service n'est pas là.

SERVED = {"balance": 2, "status": 3, "search": 3, "notifications": 2, "osk": 5}
SETTLE = 8  # secondes de lancement de la barre laissées aux greffons


def output_directory():
    return Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "winlux/tray-out"


def _panel_pid():
    """xfce4-panel le plus récent du compte (son âge remplace celui du greffon).

    Le plus récent : une barre relancée, ou une ancienne pas encore éteinte,
    ne doit pas faire croire que la barre affichée tourne depuis longtemps
    (le contrepoids serait mesuré pendant le lancement, #602)."""
    best = None
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if (entry / "comm").read_text().strip() != "xfce4-panel" or entry.stat().st_uid != os.getuid():
                continue
            age = _seconds_since_start(int(entry.name))
        except (OSError, ValueError):
            continue
        if age is not None and (best is None or age < best[1]):
            best = (int(entry.name), age)
    return best[0] if best else None


def _write_if_changed(path, text):
    try:
        if path.read_text(encoding="utf-8") == text:
            return
    except OSError:
        pass
    temporary = path.with_name(path.name + f".{os.getpid()}")
    temporary.write_text(text, encoding="utf-8")
    temporary.replace(path)


# ── Recalcul sur changement (#3129) ──────────────────────────────────────────
# Au repos, le service relançait toutes les 2 à 3 s xwininfo -root -tree, un
# xprop par greffon, xfconf-query, xdpyinfo, ip, nmcli et pactl : 8,2 % d'un
# cœur et des centaines de requêtes X sur le vrai PC (22 novembre), barre
# immobile. Le calcul lui-même ne change pas : il n'est refait que lorsque
# quelque chose qu'il lit a pu changer, et au moins toutes les FALLBACK
# secondes. Un moyen de suivi absent (pas de libX11, pas de pactl, pas de
# nmcli) laisse le greffon concerné recalculé à chaque tour, comme avant.

FALLBACK = 60          # recalcul de sûreté, même sans changement vu
WATCH_SETTLE = 30      # barre lancée depuis moins longtemps : toujours recalculé


class _XWatch:
    """Une connexion X résidente qui reçoit, sans rien demander au serveur,
    les changements de fenêtres de la barre (greffon redimensionné, icône
    ajoutée) et de l'écran (résolution). Aucun programme lancé."""

    CREATE, DESTROY, MAP, UNMAP, REPARENT, CONFIGURE, PROPERTY = 16, 17, 19, 18, 21, 22, 28

    def __init__(self):
        import ctypes
        import ctypes.util
        self.ok, self.version, self.ctypes = False, 0, ctypes
        if not os.environ.get("DISPLAY"):
            return
        name = ctypes.util.find_library("X11")
        if not name:
            return
        try:
            x = ctypes.CDLL(name)
        except OSError:
            return
        self.x = x
        x.XOpenDisplay.restype = ctypes.c_void_p
        x.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x.XDefaultRootWindow.restype = ctypes.c_ulong
        x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        x.XSelectInput.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_long]
        x.XPending.argtypes = [ctypes.c_void_p]
        x.XNextEvent.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        x.XFlush.argtypes = [ctypes.c_void_p]
        x.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
                                 ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_void_p),
                                 ctypes.POINTER(ctypes.c_uint)]
        x.XFree.argtypes = [ctypes.c_void_p]
        x.XGetClassHint.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_void_p]
        x.XConnectionNumber.argtypes = [ctypes.c_void_p]
        # Une fenêtre peut disparaître entre deux requêtes : l'erreur BadWindow
        # est ignorée, au lieu du gestionnaire par défaut qui quitte le programme.
        handler = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)
        self._handler = handler(lambda display, event: 0)
        x.XSetErrorHandler(self._handler)
        self.display = x.XOpenDisplay(None)
        if not self.display:
            return
        self.root = x.XDefaultRootWindow(self.display)
        self.event = ctypes.create_string_buffer(192)
        self.watched = set()
        # Racine : fenêtres de premier niveau (barre relancée, écran redimensionné).
        x.XSelectInput(self.display, self.root, (1 << 17) | (1 << 19))
        self.ok = True
        self.rescan()

    def _children(self, window):
        c = self.ctypes
        root, parent, children, count = c.c_ulong(), c.c_ulong(), c.c_void_p(), c.c_uint()
        if not self.x.XQueryTree(self.display, window, c.byref(root), c.byref(parent),
                                 c.byref(children), c.byref(count)):
            return []
        found = []
        if children.value:
            array = c.cast(children, c.POINTER(c.c_ulong))
            found = [array[i] for i in range(count.value)]
            self.x.XFree(children)
        return found

    def _is_panel(self, window):
        c = self.ctypes

        class Hint(c.Structure):
            _fields_ = [("name", c.c_char_p), ("klass", c.c_char_p)]
        hint = Hint()
        if not self.x.XGetClassHint(self.display, window, c.byref(hint)):
            return False
        name = hint.name or b""
        # Les chaînes appartiennent à Xlib : lues ici, puis libérées.
        for pointer in (Hint.name.offset, Hint.klass.offset):
            value = c.c_void_p.from_buffer(hint, pointer).value
            if value:
                self.x.XFree(value)
        return name == b"xfce4-panel"

    def _watch_tree(self, window):
        """Suit une fenêtre de la barre et toutes ses descendantes."""
        stack = [window]
        while stack:
            current = stack.pop()
            if current not in self.watched:
                self.watched.add(current)
                # StructureNotify + SubstructureNotify : taille, apparition, disparition.
                self.x.XSelectInput(self.display, current, (1 << 17) | (1 << 19))
            stack.extend(self._children(current))

    def rescan(self):
        """Cherche les fenêtres de la barre (au lancement, ou quand une fenêtre
        de premier niveau apparaît) ; quelques requêtes, jamais à chaque tour."""
        self.watched = {w for w in self.watched if w == self.root}
        stack = list(self._children(self.root))
        depth = {w: 1 for w in stack}
        while stack:
            window = stack.pop()
            if self._is_panel(window):
                self._watch_tree(window)
            elif depth[window] < 3:
                for child in self._children(window):
                    depth[child] = depth[window] + 1
                    stack.append(child)
        self.x.XFlush(self.display)
        self.version += 1

    def poll(self):
        """Lit les événements déjà arrivés (aucun aller-retour vers X)."""
        if not self.ok:
            return
        c, rescan, seen = self.ctypes, False, False
        while self.x.XPending(self.display):
            self.x.XNextEvent(self.display, self.event)
            kind = c.c_int.from_buffer(self.event).value
            window = c.c_ulong.from_buffer(self.event, 40).value
            parent = c.c_ulong.from_buffer(self.event, 32).value
            if parent == self.root and window != self.root:
                # Fenêtre de premier niveau (événement reçu par la racine) :
                # seule une nouvelle fenêtre peut être une barre relancée ;
                # les autres (déplacées, redimensionnées) ne changent rien ici.
                rescan = rescan or kind in (self.CREATE, self.MAP, self.REPARENT)
                continue
            seen = True
            if kind in (self.CREATE, self.REPARENT, self.MAP) and window not in self.watched:
                self._watch_tree(window)
        if rescan:
            self.rescan()
        elif seen:
            self.x.XFlush(self.display)
            self.version += 1


class _Lines:
    """Programme résident qui écrit une ligne par changement (pactl subscribe,
    nmcli monitor) : son compteur avance à chaque ligne lue, sans attente."""

    def __init__(self, args, keep=lambda line: True):
        self.version, self.keep, self.process, self.buffer = 0, keep, None, b""
        try:
            self.process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                            stdin=subprocess.DEVNULL, env=dict(os.environ, LC_ALL="C"))
            os.set_blocking(self.process.stdout.fileno(), False)
        except (OSError, ValueError):
            self.process = None

    @property
    def ok(self):
        return self.process is not None and self.process.poll() is None

    def poll(self):
        if self.process is None:
            return
        while True:
            try:
                chunk = os.read(self.process.stdout.fileno(), 65536)
            except BlockingIOError:
                break
            except OSError:
                chunk = b""
            if not chunk:
                break
            self.buffer += chunk
        *lines, self.buffer = self.buffer.split(b"\n")
        if any(self.keep(line.decode(errors="replace")) for line in lines):
            self.version += 1

    def close(self):
        if self.ok:
            self.process.terminate()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()


def _default_interface():
    """Interface de la route IPv4 par défaut, lue dans /proc (sans lancer ip)."""
    try:
        for line in Path("/proc/net/route").read_text().splitlines()[1:]:
            fields = line.split()
            if len(fields) > 7 and fields[1] == "00000000" and fields[7] == "00000000" and int(fields[3], 16) & 1:
                return fields[0]
    except (OSError, ValueError):
        return None
    return ""


def _read(path):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return ""


def _status_signature():
    """Ce que montre le bouton réseau/son/batterie et qui se lit sans programme."""
    interface = _default_interface()
    net = (interface, _read(f"/sys/class/net/{interface}/operstate") if interface else "",
           _read(f"/sys/class/net/{interface}/carrier") if interface else "")
    supplies = Path(os.environ.get("WINLUX_POWER_SUPPLY", "/sys/class/power_supply"))
    power = []
    try:
        for supply in sorted(supplies.iterdir()):
            power.append((supply.name, _read(supply / "capacity"), _read(supply / "status"), _read(supply / "online")))
    except OSError:
        pass
    return net, tuple(power)


class Changes:
    """Dit, pour chaque greffon, si son calcul peut être repris tel quel."""

    def __init__(self, enabled=True):
        self.enabled = enabled
        self.x = _XWatch() if enabled else None
        # Volume, sourdine ou sortie par défaut ; pas les flux des programmes.
        self.audio = _Lines(["pactl", "subscribe"],
                            lambda line: re.search(r" on (sink|server|card) ", line + " ") is not None) if enabled else None
        self.network = _Lines(["nmcli", "monitor"]) if enabled else None
        self.last = {}

    def poll(self):
        if not self.enabled:
            return
        for watcher in (self.x, self.audio, self.network):
            try:
                watcher.poll()
            except Exception:  # un suivi en panne : son greffon redevient recalculé à chaque tour
                if isinstance(watcher, _Lines):
                    watcher.close()
                    watcher.process = None
                else:
                    watcher.ok = False

    def key(self, kind):
        """Empreinte des entrées du greffon ; None = rien ne permet de le suivre."""
        if not self.enabled:
            return None
        settings = (_LANG, DARK, tuple(sorted(taskbar_preferences().items())))
        if kind == "balance":
            if not self.x.ok:
                return None
            # L'alignement est relu dans le fichier ; à défaut il vient de
            # xfconf, qui ne change qu'avec la barre (événement X).
            align = Path(os.environ.get("WINLUX_CONTROL_ROOT", "") + "/etc/winlux/preferences.conf")
            return settings, self.x.version, _read(align)
        if kind == "search":
            return (settings, self.x.version) if self.x.ok else None
        if kind == "status":
            if not (self.audio.ok and self.network.ok):
                return None
            return settings, self.audio.version, self.network.version, _status_signature()
        return None  # notifications : un fichier lu, rien à épargner

    def unchanged(self, kind, now, panel_age):
        """Vrai si le greffon n'a pas à être recalculé à ce tour."""
        key = self.key(kind)
        previous = self.last.get(kind)
        if key is None or previous is None or panel_age is None or panel_age < WATCH_SETTLE:
            return False
        return previous[0] == key and now - previous[1] < FALLBACK

    def done(self, kind, now):
        # Empreinte relue après le calcul : un changement survenu pendant
        # celui-ci (barre qui se réarrange) sera vu au tour suivant.
        self.poll()
        self.last[kind] = (self.key(kind), now)

    def close(self):
        for watcher in (self.audio, self.network):
            if watcher is not None:
                watcher.close()


def serve(clock=time.monotonic, sleep=time.sleep, rounds=None, watch_changes=True):
    """Boucle du service ; s'arrête sans barre des tâches pendant 20 s."""
    import fcntl
    folder = output_directory()
    folder.mkdir(parents=True, exist_ok=True)
    lock = open(folder / "serve.lock", "w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        return 0  # déjà lancé pour ce compte
    # Sorties d'une session précédente (autre écran, autre ordre de greffons) :
    # effacées, sinon le lecteur montrerait un contrepoids faux au démarrage (#602).
    for stale in folder.glob("out-*"):
        try:
            stale.unlink()
        except OSError:
            pass
    _write_if_changed(folder / "serve.pid", f"{os.getpid()}\n")
    due = {kind: 0.0 for kind in SERVED}
    missing_since = None
    changes = Changes(enabled=watch_changes)
    while rounds is None or rounds > 0:
        if rounds is not None:
            rounds -= 1
        now = clock()
        panel = _panel_pid()
        if panel is None or (_seconds_since_start(panel) or 0) < SETTLE:
            # Sorties calculées pour une barre qui n'est plus là : le lecteur
            # ne les sert plus (#2467).
            try:
                (folder / "serve.panel").unlink()
            except OSError:
                pass
        if panel is None:
            missing_since = missing_since or now
            if now - missing_since > 20:
                break
        elif (_seconds_since_start(panel) or 0) < SETTLE:
            # Barre en train de se lancer : les greffons calculent eux-mêmes
            # (lecteur sans sortie servie), comme avant le service. Deux
            # calculs en parallèle au lancement laissaient parfois une icône
            # vide pour toute la session.
            missing_since = None
        else:
            missing_since = None
            refresh_settings()
            changes.poll()
            panel_age = _seconds_since_start(panel)
            for kind, period in SERVED.items():
                if now < due[kind]:
                    continue
                due[kind] = now + period
                if changes.unchanged(kind, now, panel_age) and (folder / f"out-{kind}").exists():
                    continue
                try:
                    line = render(kind, panel)
                    changes.done(kind, now)
                except Exception as error:  # un greffon en échec ne bloque pas les autres
                    _balance_log(f"service : {kind} en échec ({error!r})")
                    continue
                if line:
                    if kind == "balance":
                        _balance_log(f"service : {line}")
                    _write_if_changed(folder / f"out-{kind}", line + "\n")
            # Barre pour laquelle ces sorties valent : le lecteur vérifie
            # qu'elle tourne encore avant de les servir.
            _write_if_changed(folder / "serve.panel", f"{panel}\n")
        sleep(1)
    changes.close()
    try:
        (folder / "serve.pid").unlink()
    except OSError:
        pass
    return 0


def overflow(argv):
    """winlux-tray overflow list|hide NOM|show NOM (zone de notification, #927)."""
    action = argv[2] if len(argv) > 2 else "list"
    if action == "list":
        print(json.dumps(overflow_list(), ensure_ascii=False))
        return 0
    if action in ("hide", "show") and len(argv) == 4 and argv[3] not in OVERFLOW_SKIP:
        return 0 if overflow_set(argv[3], action == "hide") else 1
    return 2


def main(argv):
    if len(argv) >= 2 and argv[1] == "overflow":
        return overflow(argv)
    if len(argv) != 2:
        print("usage : winlux-tray status|search|balance|edge|notifications|osk|battery|audio|network|serve", file=sys.stderr)
        return 2
    if argv[1] == "serve":
        # Lancé depuis un greffon : ne garder aucun descripteur hérité de genmon.
        os.closerange(3, 1024)
        return serve()
    line = render(argv[1])
    if line is None:
        return 2
    print(line)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
