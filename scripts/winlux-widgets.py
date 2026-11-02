#!/usr/bin/python3
"""Volet de widgets façon Windows 11 (Win+W), session native.

    winlux-widgets                 ouvre ou referme le volet
    winlux-widgets --tray          bouton météo de la barre (greffon genmon)
    winlux-widgets weather-update  relit la météo maintenant
    winlux-widgets set CLÉ VALEUR  location, units (c|f), photos, button (on|off)
    winlux-widgets reset           widgets et ordre par défaut
    winlux-widgets status          réglages en JSON (Paramètres)

Le volet glisse depuis la gauche : météo, agenda du jour, état du PC, pense-bête,
photos d'un dossier et minuteur. Pas de fil d'actualité ni de service Microsoft.
Chaque widget se retire, se déplace ou se rajoute depuis le volet ; les réglages
(bouton de la barre, lieu de la météo, dossier des photos) sont dans
Paramètres > Fond d'écran et couleurs.

La météo vient de wttr.in et s'écrit dans /run/winlux/weather.json au format
« clé=valeur » déjà lu par le widget météo du shell PE (testexe/src/shell/
weather.rs) : une seule source pour les deux bureaux.
"""
from pathlib import Path
import datetime
import html
import json
import math
import os
import re
import shutil
from string import Template
import subprocess
import sys
import threading
import time
import urllib.parse
import urllib.request


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
    'Widgets': ('Widgets', 'Widgets', 'Widgets'),
    'Météo': ('Weather', 'Tiempo', 'Wetter'),
    'Agenda du jour': ('Today’s agenda', 'Agenda de hoy', 'Heutige Termine'),
    'État du PC': ('PC status', 'Estado del equipo', 'PC-Status'),
    'Pense-bête': ('Sticky note', 'Nota adhesiva', 'Notizzettel'),
    'Photos': ('Photos', 'Fotos', 'Fotos'),
    'Minuteur': ('Timer', 'Temporizador', 'Timer'),
    'Processeur': ('Processor', 'Procesador', 'Prozessor'),
    'Mémoire': ('Memory', 'Memoria', 'Arbeitsspeicher'),
    'Batterie': ('Battery', 'Batería', 'Akku'),
    'Disque': ('Disk', 'Disco', 'Datenträger'),
    '{used} sur {total}': ('{used} of {total}', '{used} de {total}', '{used} von {total}'),
    'en charge': ('charging', 'cargando', 'wird geladen'),
    'Ressenti {value}': ('Feels like {value}', 'Sensación {value}', 'Gefühlt {value}'),
    'Humidité {value} %': ('Humidity {value}%', 'Humedad {value} %', 'Luftfeuchtigkeit {value} %'),
    'Vent {value} km/h': ('Wind {value} km/h', 'Viento {value} km/h', 'Wind {value} km/h'),
    'Ensoleillé': ('Sunny', 'Soleado', 'Sonnig'),
    'Éclaircies': ('Partly cloudy', 'Parcialmente nublado', 'Teilweise bewölkt'),
    'Nuageux': ('Cloudy', 'Nublado', 'Bewölkt'),
    'Pluie': ('Rain', 'Lluvia', 'Regen'),
    'Neige': ('Snow', 'Nieve', 'Schnee'),
    'Orage': ('Thunderstorm', 'Tormenta', 'Gewitter'),
    'Brouillard': ('Fog', 'Niebla', 'Nebel'),
    'Mis à jour à {time}': ('Updated at {time}', 'Actualizado a las {time}', 'Aktualisiert um {time}'),
    'Actualiser': ('Refresh', 'Actualizar', 'Aktualisieren'),
    'Aucun rendez-vous aujourd’hui': ('No events today', 'No hay citas hoy', 'Heute keine Termine'),
    'Ajouter : 14:30 Dentiste': ('Add: 14:30 Dentist', 'Añadir: 14:30 Dentista', 'Hinzufügen: 14:30 Zahnarzt'),
    'Supprimer': ('Delete', 'Eliminar', 'Löschen'),
    'Écrivez ici, c’est enregistré tout seul.': ('Write here, it is saved automatically.', 'Escriba aquí, se guarda solo.', 'Hier schreiben, wird automatisch gespeichert.'),
    'Démarrer': ('Start', 'Iniciar', 'Starten'),
    'Pause': ('Pause', 'Pausa', 'Pause'),
    'Reprendre': ('Resume', 'Reanudar', 'Fortsetzen'),
    'Réinitialiser': ('Reset', 'Restablecer', 'Zurücksetzen'),
    'Minuteur terminé': ('Timer finished', 'Temporizador finalizado', 'Timer abgelaufen'),
    'Le temps de {value} est écoulé.': ('{value} is up.', 'Se ha agotado el tiempo de {value}.', '{value} sind abgelaufen.'),
    'Monter': ('Move up', 'Subir', 'Nach oben'),
    'Descendre': ('Move down', 'Bajar', 'Nach unten'),
    'Retirer': ('Remove', 'Quitar', 'Entfernen'),
    'Options du widget': ('Widget options', 'Opciones del widget', 'Widget-Optionen'),
    'Ajouter des widgets': ('Add widgets', 'Añadir widgets', 'Widgets hinzufügen'),
    'Tous les widgets sont affichés.': ('All widgets are shown.', 'Se muestran todos los widgets.', 'Alle Widgets werden angezeigt.'),
    'Paramètres des widgets': ('Widget settings', 'Configuración de widgets', 'Widget-Einstellungen'),
    'Photo précédente': ('Previous photo', 'Foto anterior', 'Vorheriges Foto'),
    'Photo suivante': ('Next photo', 'Foto siguiente', 'Nächstes Foto'),
    'Ouvrir le dossier': ('Open folder', 'Abrir carpeta', 'Ordner öffnen'),
    'Météo : {value}': ('Weather: {value}', 'Tiempo: {value}', 'Wetter: {value}'),
    'Ouvrir les widgets (Windows + W)': ('Open widgets (Windows + W)', 'Abrir widgets (Windows + W)', 'Widgets öffnen (Windows + W)'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


DAYS = {"fr": "lundi mardi mercredi jeudi vendredi samedi dimanche",
        "en": "Monday Tuesday Wednesday Thursday Friday Saturday Sunday",
        "es": "lunes martes miércoles jueves viernes sábado domingo",
        "de": "Montag Dienstag Mittwoch Donnerstag Freitag Samstag Sonntag"}
SHORT_DAYS = {"fr": "lun mar mer jeu ven sam dim", "en": "Mon Tue Wed Thu Fri Sat Sun",
              "es": "lun mar mié jue vie sáb dom", "de": "Mo Di Mi Do Fr Sa So"}
MONTHS = {"fr": "janvier février mars avril mai juin juillet août septembre octobre novembre décembre",
          "en": "January February March April May June July August September October November December",
          "es": "enero febrero marzo abril mayo junio julio agosto septiembre octubre noviembre diciembre",
          "de": "Januar Februar März April Mai Juni Juli August September Oktober November Dezember"}


def today():
    """Date du jour ; WINLUX_TODAY=AAAA-MM-JJ la fige (captures reproductibles)."""
    fixed = os.environ.get("WINLUX_TODAY", "")
    try:
        return datetime.date.fromisoformat(fixed) if fixed else datetime.date.today()
    except ValueError:
        return datetime.date.today()


def clock_text():
    """Heure du volet ; WINLUX_WIDGETS_TIME=HH:MM la fige (captures)."""
    return os.environ.get("WINLUX_WIDGETS_TIME") or time.strftime("%I:%M %p" if _LANG == "en" else "%H:%M")


def long_date(day):
    weekday = DAYS[_LANG].split()[day.weekday()]
    month = MONTHS[_LANG].split()[day.month - 1]
    return {"en": f"{weekday}, {month} {day.day}", "es": f"{weekday}, {day.day} de {month}",
            "de": f"{weekday}, {day.day}. {month}"}.get(_LANG, f"{weekday} {day.day} {month}")


# ── Réglages : ~/.config/winlux/widgets.json, propre au compte ──────────────

WIDGETS = ("weather", "agenda", "pc", "notes", "photos", "timer")
TITLES = {"weather": "Météo", "agenda": "Agenda du jour", "pc": "État du PC", "notes": "Pense-bête",
          "photos": "Photos", "timer": "Minuteur"}
ICONS = {"weather": "weather-few-clouds-symbolic", "agenda": "x-office-calendar-symbolic",
         "pc": "computer-symbolic", "notes": "accessories-text-editor-symbolic",
         "photos": "image-x-generic-symbolic", "timer": "alarm-symbolic"}
DEFAULTS = {"order": list(WIDGETS), "hidden": [], "location": "", "units": "c", "photos": "",
            "button": False, "notes": "", "agenda": []}


def config_path():
    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "winlux/widgets.json"


def load_config():
    data = dict(DEFAULTS)
    try:
        stored = json.loads(config_path().read_text(encoding="utf-8"))
        if isinstance(stored, dict):
            data.update({key: value for key, value in stored.items() if key in DEFAULTS})
    except (OSError, ValueError):
        pass
    # Ordre toujours complet et sans doublon, même après une mise à jour qui
    # ajoute un widget : les nouveaux se rangent à la fin.
    order = [name for name in data.get("order") or [] if name in WIDGETS]
    data["order"] = list(dict.fromkeys(order + [name for name in WIDGETS if name not in order]))
    data["hidden"] = [name for name in data.get("hidden") or [] if name in WIDGETS]
    data["units"] = "f" if data.get("units") == "f" else "c"
    # Rendez-vous passés effacés : l'agenda ne garde que le jour et la suite.
    now = today().isoformat()
    data["agenda"] = [entry for entry in data.get("agenda") or []
                      if isinstance(entry, dict) and str(entry.get("date", "")) >= now and entry.get("text")]
    return data


def save_config(data):
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + f".{os.getpid()}.tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=1), encoding="utf-8")
    temporary.replace(path)


def move(data, name, step):
    order = data["order"]
    index = order.index(name)
    target = max(0, min(len(order) - 1, index + step))
    order.insert(target, order.pop(index))
    return data


def parse_event(text, day=None):
    """« 14:30 Dentiste », « 9h Réunion » ou « Appeler Paul » -> rendez-vous."""
    text = " ".join(text.split())
    found = re.match(r"^(\d{1,2})(?:[:hH.](\d{2})?)\s+(.+)$", text)
    when = ""
    if found and int(found[1]) < 24 and int(found[2] or 0) < 60:
        when, text = f"{int(found[1]):02d}:{int(found[2] or 0):02d}", found[3]
    return {"date": (day or today()).isoformat(), "time": when, "text": text[:120]} if text else None


def events_of(data, day=None):
    key = (day or today()).isoformat()
    return sorted((entry for entry in data["agenda"] if entry.get("date") == key),
                  key=lambda entry: (entry.get("time") or "99:99", entry.get("text", "")))


# ── État du PC (racine WINLUX_WIDGETS_ROOT pour les tests) ──────────────────

ROOT = Path(os.environ.get("WINLUX_WIDGETS_ROOT", "/"))


def cpu_times():
    try:
        fields = (ROOT / "proc/stat").read_text().splitlines()[0].split()[1:]
        values = [int(value) for value in fields]
        idle = values[3] + (values[4] if len(values) > 4 else 0)
        return sum(values), idle
    except (OSError, IndexError, ValueError):
        return None


def cpu_percent(previous, current):
    if not previous or not current or current[0] <= previous[0]:
        return None
    total, idle = current[0] - previous[0], current[1] - previous[1]
    return max(0, min(100, round(100 * (total - idle) / total)))


def memory():
    """(utilisée, totale) en octets d'après /proc/meminfo, ou None."""
    values = {}
    try:
        for line in (ROOT / "proc/meminfo").read_text().splitlines():
            key, _, rest = line.partition(":")
            values[key] = int(rest.split()[0]) * 1024
    except (OSError, ValueError, IndexError):
        return None
    total = values.get("MemTotal")
    available = values.get("MemAvailable", values.get("MemFree"))
    return (total - available, total) if total and available is not None else None


def battery():
    """(niveau, en charge) de la première batterie réelle, ou None (masqué)."""
    for capacity in sorted((ROOT / "sys/class/power_supply").glob("BAT*/capacity")):
        try:
            level = int(capacity.read_text().strip())
            status = (capacity.parent / "status").read_text().strip() if (capacity.parent / "status").is_file() else ""
        except (OSError, ValueError):
            continue
        if 0 <= level <= 100:
            return level, status == "Charging"
    return None


def disk(path=None):
    try:
        usage = shutil.disk_usage(path or os.environ.get("WINLUX_WIDGETS_DISK") or Path.home())
        return usage.used, usage.total
    except OSError:
        return None


def size_text(value):
    units = {"fr": ("o", "Ko", "Mo", "Go", "To"), "es": ("B", "KB", "MB", "GB", "TB"),
             "de": ("B", "KB", "MB", "GB", "TB"), "en": ("B", "KB", "MB", "GB", "TB")}[_LANG]
    size, index = float(value), 0
    while size >= 1024 and index < len(units) - 1:
        size, index = size / 1024, index + 1
    number = f"{size:.0f}" if size >= 100 or index == 0 else f"{size:.1f}"
    return (number if _LANG == "en" else number.replace(".", ",")) + " " + units[index]


# ── Météo : même fichier que le widget du shell PE ──────────────────────────

RUN = Path(os.environ.get("WINLUX_RUN_DIR", "/run/winlux"))
WEATHER_MAX_AGE = 30 * 60
CONDITIONS = {"sunny": "Ensoleillé", "partly": "Éclaircies", "cloudy": "Nuageux", "rain": "Pluie",
              "snow": "Neige", "storm": "Orage", "fog": "Brouillard"}
# Codes météo de wttr.in (ceux de WWO) regroupés comme dans weather.rs.
_CODES = {"sunny": {113}, "partly": {116}, "cloudy": {119, 122}, "fog": {143, 248, 260},
          "storm": {200, 386, 389, 392, 395},
          "snow": {179, 182, 185, 227, 230, 281, 284, 311, 314, 317, 320, 323, 326, 329, 332, 335, 338,
                   350, 362, 365, 368, 371, 374, 377}}


def condition_of(code):
    try:
        code = int(code)
    except (TypeError, ValueError):
        return "cloudy"
    return next((name for name, codes in _CODES.items() if code in codes), "rain")


def weather_paths():
    """Fichier partagé (/run/winlux, ouvert à tous) puis cache du compte."""
    cache = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "winlux/weather.json"
    return [RUN / "weather.json", cache]


def read_weather():
    """Dernier relevé : dict (temp, condition, forecast…) ou None."""
    best = None
    for path in weather_paths():
        try:
            text = path.read_bytes().decode("cp1252", errors="replace")
        except OSError:
            continue
        values = dict(line.split("=", 1) for line in text.splitlines() if "=" in line)
        if "temp" not in values:
            continue
        try:
            updated = int(values.get("updated", "0"))
        except ValueError:
            updated = 0
        if best is None or updated > best["updated"]:
            def number(key):
                try: return int(values.get(key, ""))
                except ValueError: return None
            forecast = []
            for index in range(3):
                low, high = number(f"forecast{index}_min"), number(f"forecast{index}_max")
                if low is None or high is None:
                    break
                forecast.append({"day": values.get(f"forecast{index}_day", ""), "min": low, "max": high,
                                 "condition": values.get(f"forecast{index}_cond", "cloudy")})
            best = {"temp": number("temp"), "feels_like": number("feels_like"), "humidity": number("humidity"),
                    "wind": number("wind"), "condition": values.get("condition", "cloudy"),
                    "city": values.get("city", ""), "units": values.get("units", "c"),
                    "updated": updated, "forecast": forecast}
    return best


def weather_lines(report, units, lang=None):
    """wttr.in (format j1) -> lignes « clé=valeur » lisibles par weather.rs."""
    lang = lang or _LANG
    current = report["current_condition"][0]
    fahrenheit = units == "f"
    area = (report.get("nearest_area") or [{}])[0]
    city = ((area.get("areaName") or [{}])[0].get("value") or "").replace("\n", " ")
    lines = {"temp": current["temp_F" if fahrenheit else "temp_C"],
             "feels_like": current["FeelsLikeF" if fahrenheit else "FeelsLikeC"],
             "humidity": current.get("humidity", "0"),
             "wind": current.get("windspeedKmph", "0"),
             "condition": condition_of(current.get("weatherCode")),
             "city": city, "units": units, "updated": str(int(time.time()))}
    for index, day in enumerate((report.get("weather") or [])[:3]):
        try:
            date = datetime.date.fromisoformat(day["date"])
            name = SHORT_DAYS[lang].split()[date.weekday()]
        except (KeyError, ValueError):
            name = ""
        hourly = day.get("hourly") or []
        noon = hourly[len(hourly) // 2] if hourly else {}
        lines.update({f"forecast{index}_day": name,
                      f"forecast{index}_min": day["mintempF" if fahrenheit else "mintempC"],
                      f"forecast{index}_max": day["maxtempF" if fahrenheit else "maxtempC"],
                      f"forecast{index}_cond": condition_of(noon.get("weatherCode"))})
    return "".join(f"{key}={value}\n" for key, value in lines.items())


def fetch_weather(data=None, timeout=8):
    """Interroge wttr.in (lieu des réglages, sinon d'après la connexion) et
    écrit le relevé. Renvoie True si un relevé a été écrit."""
    if os.environ.get("WINLUX_WEATHER_OFFLINE") == "1":
        return False
    data = data or load_config()
    location = urllib.parse.quote(data.get("location", "").strip())
    lang = {"fr": "fr", "es": "es", "de": "de"}.get(_LANG, "en")
    url = os.environ.get("WINLUX_WEATHER_URL", "https://wttr.in/{location}?format=j1&lang={lang}").format(
        location=location, lang=lang)
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "winlux-widgets"})
        with urllib.request.urlopen(request, timeout=timeout) as answer:
            report = json.loads(answer.read(200_000).decode("utf-8"))
        text = weather_lines(report, data.get("units", "c"))
    except (OSError, ValueError, KeyError, IndexError, TypeError):
        return False
    written = False
    for path in weather_paths():
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary = path.with_name(path.name + f".{os.getpid()}.tmp")
            temporary.write_bytes(text.encode("cp1252", errors="replace"))
            temporary.chmod(0o644)
            temporary.replace(path)
            written = True
            break  # /run/winlux suffit ; le cache sert quand il est fermé
        except OSError:
            continue
    return written


def weather_stale(report):
    return report is None or time.time() - report.get("updated", 0) > WEATHER_MAX_AGE


def temperature(value, units):
    return "–" if value is None else f"{value}°" + ("F" if units == "f" else "")


# ── Bouton de la barre des tâches (greffon genmon n° 11) ────────────────────

PLUGIN = 11


def _xfconf(*args):
    try:
        result = subprocess.run(["xfconf-query", "-c", "xfce4-panel", *args], capture_output=True, text=True, timeout=5)
        return result.stdout if result.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


def panel_ids():
    output = _xfconf("-p", "/panels/panel-1/plugin-ids") or ""
    return [int(line) for line in output.splitlines() if line.strip().isdigit()]


def ids_with_button(ids, centered):
    """Windows 11 : à gauche de la barre centrée, à droite de la barre à gauche."""
    ids = [value for value in ids if value != PLUGIN]
    if centered:
        return [PLUGIN] + ids
    tray = next((index for index, value in enumerate(ids) if value in (10, 4)), len(ids))
    return ids[:tray] + [PLUGIN] + ids[tray:]


def set_button(enabled):
    """Ajoute ou retire le bouton dans la barre en cours (xfconf)."""
    if not shutil.which("xfconf-query"):
        return False
    ids = panel_ids()
    if not ids:
        return False
    if enabled:
        for prop, kind, value in (("", "string", "genmon"), ("/command", "string", "/usr/bin/winlux-widgets --tray"),
                                  ("/use-label", "bool", "false"), ("/text", "string", ""),
                                  ("/update-period", "int", "600000"), ("/font", "string", "DejaVu Sans 10")):
            path = f"/plugins/plugin-{PLUGIN}{prop}"
            if _xfconf("-p", path, "-s", value) is None:
                _xfconf("-p", path, "-n", "-t", kind, "-s", value)
        centered = (_xfconf("-p", "/plugins/plugin-3/expand") or "true").strip() != "false"
        wanted = ids_with_button(ids, centered)
    else:
        wanted = [value for value in ids if value != PLUGIN]
    if wanted != ids:
        args = ["-p", "/panels/panel-1/plugin-ids", "-a"]
        for value in wanted:
            args += ["-t", "int", "-s", str(value)]
        _xfconf(*args)
    if not enabled:
        _xfconf("-p", f"/plugins/plugin-{PLUGIN}", "-r", "-R")
    return True


def tray_icon(condition):
    """Petite icône SVG de la condition, claire sur la barre sombre."""
    color = "#f3f0fa"
    sun = f'<circle cx="12" cy="10" r="4.5" fill="#ffc83d"/>'
    cloud = f'<path d="M7 19h10a4 4 0 0 0 0-8 5.5 5.5 0 0 0-10.5 1.5A3.3 3.3 0 0 0 7 19z" fill="{color}"/>'
    body = {"sunny": '<circle cx="12" cy="12" r="5" fill="#ffc83d"/>' + "".join(
                f'<path d="M{12 + 8 * math.cos(a):.1f} {12 + 8 * math.sin(a):.1f}L{12 + 10.5 * math.cos(a):.1f} {12 + 10.5 * math.sin(a):.1f}" stroke="#ffc83d" stroke-width="1.6"/>'
                for a in (i * math.pi / 4 for i in range(8))),
            "partly": sun + cloud, "cloudy": cloud,
            "rain": cloud + f'<path d="M9 20l-1 3m4-3l-1 3m4-3l-1 3" stroke="#7fb8ff" stroke-width="1.6"/>',
            "snow": cloud + '<circle cx="9" cy="22" r="1" fill="#ffffff"/><circle cx="13" cy="22" r="1" fill="#ffffff"/>',
            "storm": cloud + '<path d="M12 18l-2 4h3l-1 2" stroke="#ffc83d" stroke-width="1.6" fill="none"/>',
            "fog": f'<path d="M4 9h16M3 13h18M5 17h14" stroke="{color}" stroke-width="2"/>'}.get(condition, cloud)
    directory = RUN / "tray"
    try:
        directory.mkdir(parents=True, exist_ok=True)
    except OSError:
        directory = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "winlux/tray"
        directory.mkdir(parents=True, exist_ok=True)
    icon = directory / f"weather-{condition}.svg"
    if not icon.exists():
        icon.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">{body}</svg>')
    return icon


def tray():
    """Sortie genmon : icône et température, clic = volet."""
    report = read_weather()
    if weather_stale(report) and fetch_weather(timeout=4):
        report = read_weather()
    command = "/usr/bin/winlux-widgets" if Path("/usr/bin/winlux-widgets").exists() else f"python3 {Path(__file__).resolve()}"
    if report:
        units = report.get("units", "c")
        label = CONDITIONS.get(report["condition"], "Nuageux")
        tooltip = _t("Météo : {value}").format(value=f"{temperature(report['temp'], units)} {_t(label)}"
                                               + (f" · {report['city']}" if report["city"] else ""))
        text = temperature(report["temp"], "c")
        print(f"<img>{tray_icon(report['condition'])}</img><txt><span foreground=\"#f3f0fa\">{html.escape(text)}</span></txt>"
              f"<click>{command}</click><txtclick>{command}</txtclick><tool>{html.escape(tooltip)}</tool>")
    else:
        print(f"<img>{tray_icon('partly')}</img><click>{command}</click>"
              f"<tool>{html.escape(_t('Ouvrir les widgets (Windows + W)'))}</tool>")


# ── Photos ──────────────────────────────────────────────────────────────────

IMAGE_SUFFIXES = {".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp"}


def pictures_dir():
    try:
        for line in (Path.home() / ".config/user-dirs.dirs").read_text(encoding="utf-8").splitlines():
            if line.startswith("XDG_PICTURES_DIR="):
                path = Path(line.split("=", 1)[1].strip().strip('"').replace("$HOME", str(Path.home())))
                if path.is_dir():
                    return path
    except OSError:
        pass
    for name in ("Images", "Pictures", "Imágenes", "Bilder"):
        if (Path.home() / name).is_dir():
            return Path.home() / name
    return Path.home() / "Images"


def photo_files(data):
    folder = Path(data.get("photos") or pictures_dir())
    try:
        files = sorted(path for path in folder.iterdir()
                       if path.suffix.lower() in IMAGE_SUFFIXES and path.is_file())
    except OSError:
        return folder, []
    return folder, files[:300]


# ── Ligne de commande sans interface graphique ──────────────────────────────

def command_line(argv):
    if not argv:
        return None
    name = argv[0]
    if name == "--tray":
        tray()
        return 0
    if name == "weather-update":
        return 0 if fetch_weather() else 1
    if name == "status":
        data = load_config()
        print(json.dumps({key: data[key] for key in ("order", "hidden", "location", "units", "photos", "button")},
                         ensure_ascii=False))
        return 0
    if name == "reset":
        data = load_config()
        data.update(order=list(WIDGETS), hidden=[])
        save_config(data)
        return 0
    if name == "set" and len(argv) == 3:
        key, value = argv[1], argv[2]
        data = load_config()
        if key == "location":
            data["location"] = value.strip()[:80]
        elif key == "units" and value in ("c", "f"):
            data["units"] = value
        elif key == "photos":
            data["photos"] = value
        elif key == "button" and value in ("on", "off"):
            data["button"] = value == "on"
            set_button(data["button"])
        else:
            print(f"Réglage inconnu : {key} {value}", file=sys.stderr)
            return 2
        save_config(data)
        if key in ("location", "units"):
            # Ancien relevé d'un autre lieu ou dans l'autre unité : refait.
            for path in weather_paths():
                try: path.unlink()
                except OSError: pass
            fetch_weather(data)
        return 0
    if name in ("-h", "--help"):
        print(__doc__)
        return 0
    return None


if __name__ == "__main__":
    handled = command_line(sys.argv[1:])
    if handled is not None:
        raise SystemExit(handled)


# ── Interface ───────────────────────────────────────────────────────────────

import gi  # noqa: E402  (la ligne de commande et la barre n'ont pas besoin de GTK)
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
gi.require_version("Pango", "1.0")
from gi.repository import Gtk, Gdk, GdkPixbuf, Gio, GLib, Pango  # noqa: E402
try:
    import cairo  # python3-gi-cairo : coins arrondis sans compositeur
except ImportError:
    cairo = None

# Même palette que winlux-popup.py et le menu Démarrer (contraste vérifié par
# test-shell-contrast.py).
PALETTES = {
    "dark": {"bg": "#242238", "fg": "#f7f5ff", "border": "#6b5f92", "dim": "#c4bee0", "button": "#322e4a",
             "hover": "#433b60", "accent": "#b19af9", "on_accent": "#1b1530", "trough": "#4a4466",
             "card": "#2e2b45", "note": "#4a4130", "note_fg": "#fff6dc"},
    "light": {"bg": "#f6f4fc", "fg": "#1f1b2e", "border": "#c9c0e6", "dim": "#5e5778", "button": "#ffffff",
              "hover": "#e4ddf7", "accent": "#6a3df0", "on_accent": "#ffffff", "trough": "#d9d2ee",
              "card": "#ffffff", "note": "#fff4c2", "note_fg": "#3b3000"},
}
CSS = Template("""
  #winlux-widgets { background: $bg; color: $fg; }
  #winlux-widgets .panel { background: $bg; color: $fg; border: 1px solid $border; border-radius: 8px; }
  #winlux-widgets box, #winlux-widgets grid, #winlux-widgets scrolledwindow, #winlux-widgets viewport,
  #winlux-widgets eventbox { background: transparent; border: none; }
  #winlux-widgets label, #winlux-widgets image { color: $fg; text-shadow: none; }
  #winlux-widgets .title { font-weight: bold; }
  #winlux-widgets .clock { font-size: 22px; font-weight: bold; }
  #winlux-widgets .big { font-size: 30px; font-weight: bold; }
  #winlux-widgets .dim, #winlux-widgets .dim label { color: $dim; }
  #winlux-widgets .card { background: $card; border: 1px solid $border; border-radius: 8px; }
  #winlux-widgets button { padding: 5px 10px; border-radius: 5px; background: $button; color: $fg;
                           border: 1px solid $border; box-shadow: none; text-shadow: none; }
  #winlux-widgets button label { color: $fg; }
  #winlux-widgets button:hover { background: $hover; }
  #winlux-widgets button.flat { background: transparent; border-color: transparent; }
  #winlux-widgets button.flat:hover { background: $hover; }
  #winlux-widgets button.icon { padding: 4px; min-width: 20px; min-height: 20px; }
  #winlux-widgets button.accent { background: $accent; border-color: $accent; }
  #winlux-widgets button.accent label { color: $on_accent; }
  #winlux-widgets entry { background: $button; color: $fg; border: 1px solid $border; border-radius: 5px;
                          caret-color: $fg; box-shadow: none; }
  #winlux-widgets entry placeholder, #winlux-widgets entry text placeholder { color: $dim; }
  #winlux-widgets levelbar trough { min-height: 6px; background: $trough; border: none; border-radius: 3px; padding: 0; }
  #winlux-widgets levelbar block.filled { background: $accent; border: none; border-radius: 3px; }
  #winlux-widgets levelbar block.empty { background: transparent; border: none; }
  #winlux-widgets .note, #winlux-widgets .note text { background: $note; color: $note_fg; caret-color: $note_fg; }
  #winlux-widgets .note { border-radius: 6px; padding: 6px; }
""")


def dark_theme():
    forced = os.environ.get("GTK_THEME")
    if forced:
        return "dark" in forced.casefold()  # GTK ignore alors la préférence
    settings = Gtk.Settings.get_default()
    name = (settings.get_property("gtk-theme-name") or "").casefold()
    return bool(settings.get_property("gtk-application-prefer-dark-theme")) or "dark" in name


def rounded(region, x, y, width, height, radius):
    """Ajoute un rectangle aux coins arrondis à une région cairo (ligne à ligne)."""
    if width <= 0 or height <= 0:
        return
    radius = min(radius, width // 2, height // 2)
    region.union(cairo.RectangleInt(x, y + radius, width, height - 2 * radius))
    for row in range(radius):
        inset = radius - int(round(math.sqrt(radius * radius - (radius - row - 0.5) ** 2)))
        region.union(cairo.RectangleInt(x + inset, y + row, width - 2 * inset, 1))
        region.union(cairo.RectangleInt(x + inset, y + height - 1 - row, width - 2 * inset, 1))


def label(text="", *classes, **props):
    props.setdefault("xalign", 0)
    widget = Gtk.Label(label=text, **props)
    for name in classes:
        widget.get_style_context().add_class(name)
    return widget


def icon_button(icon, tooltip, callback, *classes):
    button = Gtk.Button.new_from_icon_name(icon, Gtk.IconSize.BUTTON)
    for name in ("icon", "flat") + classes:
        button.get_style_context().add_class(name)
    button.set_tooltip_text(tooltip)
    button.get_accessible().set_name(tooltip)
    button.connect("clicked", lambda *_: callback())
    return button


def duration_text(seconds):
    seconds = max(0, int(round(seconds)))
    hours, rest = divmod(seconds, 3600)
    return f"{hours}:{rest // 60:02d}:{rest % 60:02d}" if hours else f"{rest // 60:02d}:{rest % 60:02d}"


class Widgets(Gtk.Application):
    WIDTH = 440

    def __init__(self):
        super().__init__(application_id="org.winlux.Widgets", flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.window = None
        self.data = load_config()
        self.cpu_previous = cpu_times()
        self.timer_total = 5 * 60
        self.timer_end = None      # instant de fin quand il tourne
        self.timer_left = self.timer_total
        self.photo_index = 0
        self.photo_tick = 0
        self.save_source = 0
        self.fetching = False
        self.anchor = None
        self.slide = None
        self.pointer_status = None
        self.held = False

    # ── Ouverture, ancrage et fermeture ─────────────────────────────────────

    def do_activate(self):
        if self.window and self.window.get_visible():
            self.hide()
            return
        if not self.window:
            self.build()
        self.data = load_config()
        self.render()
        display = Gdk.Display.get_default()
        monitor = display.get_monitor_at_point(*display.get_default_seat().get_pointer().get_position()[1:])
        self.anchor = (monitor.get_workarea(), monitor.get_geometry())
        area = self.anchor[0]
        height = max(360, area.height - 24)
        self.frame.set_size_request(min(self.WIDTH, area.width - 24), height)
        self.window.resize(min(self.WIDTH, area.width - 24), height)
        self.window.move(area.x - self.WIDTH, area.y + 12)
        self.window.show_all()
        self.render()
        self.window.present()
        self.start_slide()
        GLib.idle_add(self.grab)
        report = read_weather()
        if weather_stale(report):
            self.refresh_weather()

    def hide(self, *_):
        if self.window.has_grab():
            self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        self.window.hide()
        self.save_now()
        return True

    def grab(self):
        if self.window.get_visible():
            self.pointer_status = self.window.get_display().get_default_seat().grab(
                self.window.get_window(), Gdk.SeatCapabilities.POINTER, False, None, None, None, None)
            if self.pointer_status == Gdk.GrabStatus.SUCCESS:
                self.window.grab_add()
        return False

    def outside(self, _, event):
        x, y = self.window.get_position()
        width, height = self.window.get_size()
        if not (x <= event.x_root < x + width and y <= event.y_root < y + height):
            return self.hide()
        return False

    def target_x(self):
        return self.anchor[0].x + 12

    def start_slide(self):
        """Glisse depuis le bord gauche, comme le volet de Windows 11."""
        if os.environ.get("WINLUX_WIDGETS_NO_ANIMATION") == "1":
            self.window.move(self.target_x(), self.anchor[0].y + 12)
            return
        start, steps = self.anchor[0].x - self.WIDTH, 8
        self.slide = 0

        def step():
            if not self.window.get_visible() or self.slide is None:
                return False
            self.slide += 1
            progress = 1 - (1 - self.slide / steps) ** 3
            self.window.move(round(start + (self.target_x() - start) * progress), self.anchor[0].y + 12)
            if self.slide >= steps:
                self.slide = None
                return False
            return True
        GLib.timeout_add(16, step)

    def shape(self, *_):
        if cairo is None or not self.frame.get_visible():
            return
        allocation = self.frame.get_allocation()
        region = cairo.Region()
        rounded(region, allocation.x, allocation.y, allocation.width, allocation.height, 8)
        self.window.shape_combine_region(region)

    def apply_theme(self):
        self.css.load_from_data(CSS.substitute(PALETTES["dark" if dark_theme() else "light"]).encode())

    def build(self):
        self.window = Gtk.ApplicationWindow(application=self, title=_t("Widgets"))
        self.window.set_name("winlux-widgets")
        self.css = Gtk.CssProvider()
        Gtk.StyleContext.add_provider_for_screen(self.window.get_screen(), self.css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1)
        settings = Gtk.Settings.get_default()
        for name in ("gtk-theme-name", "gtk-application-prefer-dark-theme"):
            settings.connect("notify::" + name, lambda *_: self.apply_theme())
        self.apply_theme()
        self.window.set_decorated(False)
        self.window.set_resizable(False)
        self.window.set_skip_taskbar_hint(True)
        self.window.set_skip_pager_hint(True)
        self.window.set_type_hint(Gdk.WindowTypeHint.UTILITY)
        self.window.set_keep_above(True)
        self.window.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        self.window.connect("button-press-event", self.outside)
        self.window.connect("key-press-event", lambda _, event: self.hide() if event.keyval == Gdk.KEY_Escape else False)
        self.window.connect("focus-out-event", lambda *_: self.hide())
        self.window.connect("delete-event", self.hide)
        self.frame = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.frame.get_style_context().add_class("panel")
        self.window.add(self.frame)
        header = Gtk.Box(spacing=8, margin=16, margin_bottom=8)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.clock = label("", "clock")
        self.date = label("", "dim")
        texts.pack_start(self.clock, False, False, 0)
        texts.pack_start(self.date, False, False, 0)
        header.pack_start(texts, True, True, 0)
        header.pack_end(icon_button("emblem-system-symbolic", _t("Paramètres des widgets"), self.open_settings), False, False, 0)
        header.pack_end(icon_button("list-add-symbolic", _t("Ajouter des widgets"), self.toggle_add), False, False, 0)
        self.frame.pack_start(header, False, False, 0)
        self.add_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6, margin_start=16, margin_end=16)
        self.add_box.set_no_show_all(True)
        self.frame.pack_start(self.add_box, False, False, 0)
        scroller = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
        self.list = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=16, margin_top=8)
        scroller.add(self.list)
        self.frame.pack_start(scroller, True, True, 0)
        self.window.connect("size-allocate", self.shape)
        self.cards = {}
        GLib.timeout_add(1000, self.tick)

    # ── Construction des widgets ────────────────────────────────────────────

    def render(self):
        for child in self.list.get_children():
            self.list.remove(child)
        self.cards = {}
        self.clock.set_text(clock_text())
        self.date.set_text(long_date(today()))
        order = [name for name in self.data["order"] if name not in self.data["hidden"]]
        for index, name in enumerate(order):
            content = getattr(self, "build_" + name)()
            if content is None:
                continue  # rien à montrer (pas de photo, pas de météo hors ligne)
            card = self.card(name, content, index, len(order))
            self.cards[name] = card
            self.list.pack_start(card, False, False, 0)
        self.render_add()
        self.list.show_all()
        self.update_pc()
        self.update_timer()

    def card(self, name, content, index, count):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.get_style_context().add_class("card")
        inner = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=12)
        box.pack_start(inner, True, True, 0)
        title = Gtk.Box(spacing=8)
        title.pack_start(Gtk.Image.new_from_icon_name(ICONS[name], Gtk.IconSize.MENU), False, False, 0)
        title.pack_start(label(_t(TITLES[name]), "title"), True, True, 0)
        options = Gtk.Revealer(transition_type=Gtk.RevealerTransitionType.NONE)
        actions = Gtk.Box(spacing=6)
        for text, callback, sensitive in ((_t("Monter"), lambda: self.move(name, -1), index > 0),
                                          (_t("Descendre"), lambda: self.move(name, 1), index < count - 1),
                                          (_t("Retirer"), lambda: self.remove(name), True)):
            button = Gtk.Button(label=text)
            button.set_sensitive(sensitive)
            button.connect("clicked", lambda *_, cb=callback: cb())
            actions.pack_start(button, False, False, 0)
        options.add(actions)
        title.pack_end(icon_button("view-more-symbolic", _t("Options du widget"),
                                   lambda: options.set_reveal_child(not options.get_reveal_child())), False, False, 0)
        inner.pack_start(title, False, False, 0)
        inner.pack_start(options, False, False, 0)
        inner.pack_start(content, False, False, 0)
        box.winlux_name = name
        return box

    def render_add(self):
        for child in self.add_box.get_children():
            self.add_box.remove(child)
        hidden = [name for name in self.data["order"] if name in self.data["hidden"]]
        self.add_box.pack_start(label(_t("Ajouter des widgets"), "title"), False, False, 0)
        if not hidden:
            self.add_box.pack_start(label(_t("Tous les widgets sont affichés."), "dim"), False, False, 0)
        flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=3, column_spacing=6, row_spacing=6)
        for name in hidden:
            button = Gtk.Button()
            inner = Gtk.Box(spacing=6)
            inner.pack_start(Gtk.Image.new_from_icon_name(ICONS[name], Gtk.IconSize.MENU), False, False, 0)
            inner.pack_start(Gtk.Label(label=_t(TITLES[name])), False, False, 0)
            button.add(inner)
            button.connect("clicked", lambda *_, n=name: self.restore(n))
            flow.add(button)
        self.add_box.pack_start(flow, False, False, 0)

    def toggle_add(self):
        if self.add_box.get_visible():
            self.add_box.hide()
        else:
            self.render_add()
            self.add_box.set_no_show_all(False)
            self.add_box.show_all()
            self.add_box.set_no_show_all(True)

    def move(self, name, step):
        move(self.data, name, step)
        save_config(self.data)
        self.render()

    def remove(self, name):
        if name not in self.data["hidden"]:
            self.data["hidden"].append(name)
        save_config(self.data)
        self.render()

    def restore(self, name):
        self.data["hidden"] = [item for item in self.data["hidden"] if item != name]
        # Rajouté en tête, comme sous Windows 11.
        self.data["order"].remove(name)
        self.data["order"].insert(0, name)
        save_config(self.data)
        self.render()
        if self.add_box.get_visible():
            self.toggle_add()
            self.toggle_add()

    def open_settings(self):
        self.hide()
        try:
            subprocess.Popen(["winlux-shell-app", "settings", "21"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError:
            pass

    # Météo
    def build_weather(self):
        report = read_weather()
        if report is None:
            return None
        units = report.get("units", "c")
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        line = Gtk.Box(spacing=12)
        condition = report["condition"] if report["condition"] in CONDITIONS else "cloudy"
        icon = Gtk.Image.new_from_file(str(tray_icon(condition)))
        try:
            icon.set_from_pixbuf(GdkPixbuf.Pixbuf.new_from_file_at_size(str(tray_icon(condition)), 56, 56))
        except GLib.Error:
            pass
        line.pack_start(icon, False, False, 0)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        texts.pack_start(label(temperature(report["temp"], units), "big"), False, False, 0)
        texts.pack_start(label(_t(CONDITIONS[condition]) + (f" · {report['city']}" if report["city"] else "")), False, False, 0)
        line.pack_start(texts, True, True, 0)
        details = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        if report["feels_like"] is not None:
            details.pack_start(label(_t("Ressenti {value}").format(value=temperature(report["feels_like"], units)), "dim"), False, False, 0)
        if report["humidity"] is not None:
            details.pack_start(label(_t("Humidité {value} %").format(value=report["humidity"]), "dim"), False, False, 0)
        if report["wind"] is not None:
            details.pack_start(label(_t("Vent {value} km/h").format(value=report["wind"]), "dim"), False, False, 0)
        line.pack_end(details, False, False, 0)
        box.pack_start(line, False, False, 0)
        if report["forecast"]:
            days = Gtk.Box(spacing=8, homogeneous=True)
            for day in report["forecast"]:
                column = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
                column.pack_start(label(day["day"], "dim", xalign=0.5), False, False, 0)
                try:
                    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_size(str(tray_icon(day["condition"] if day["condition"] in CONDITIONS else "cloudy")), 28, 28)
                    column.pack_start(Gtk.Image.new_from_pixbuf(pixbuf), False, False, 0)
                except GLib.Error:
                    pass
                column.pack_start(label(f"{temperature(day['max'], 'c')} / {temperature(day['min'], 'c')}", xalign=0.5), False, False, 0)
                days.pack_start(column, True, True, 0)
            box.pack_start(days, False, False, 0)
        footer = Gtk.Box(spacing=6)
        updated = time.strftime("%H:%M", time.localtime(report["updated"])) if report["updated"] else "–"
        footer.pack_start(label(_t("Mis à jour à {time}").format(time=updated), "dim"), True, True, 0)
        footer.pack_end(icon_button("view-refresh-symbolic", _t("Actualiser"), self.refresh_weather), False, False, 0)
        box.pack_start(footer, False, False, 0)
        return box

    def refresh_weather(self):
        if self.fetching:
            return
        self.fetching = True

        def work():
            fetch_weather(self.data)
            GLib.idle_add(done)

        def done():
            self.fetching = False
            if self.window and self.window.get_visible():
                self.render()
            return False
        threading.Thread(target=work, daemon=True).start()

    # Agenda du jour
    def build_agenda(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        box.pack_start(label(long_date(today()), "dim"), False, False, 0)
        events = events_of(self.data)
        if not events:
            box.pack_start(label(_t("Aucun rendez-vous aujourd’hui")), False, False, 0)
        for entry in events:
            line = Gtk.Box(spacing=8)
            line.pack_start(label(entry.get("time") or "·", "title", width_chars=5), False, False, 0)
            line.pack_start(label(entry["text"], wrap=True), True, True, 0)
            line.pack_end(icon_button("window-close-symbolic", _t("Supprimer"), lambda e=entry: self.delete_event(e)), False, False, 0)
            box.pack_start(line, False, False, 0)
        entry = Gtk.Entry(placeholder_text=_t("Ajouter : 14:30 Dentiste"))
        entry.get_accessible().set_name(_t("Ajouter : 14:30 Dentiste"))
        entry.connect("activate", self.add_event)
        box.pack_start(entry, False, False, 0)
        return box

    def add_event(self, entry):
        event = parse_event(entry.get_text())
        if event:
            self.data["agenda"].append(event)
            save_config(self.data)
            self.render()

    def delete_event(self, event):
        self.data["agenda"] = [item for item in self.data["agenda"] if item is not event and item != event]
        save_config(self.data)
        self.render()

    # État du PC
    def build_pc(self):
        box = Gtk.Grid(column_spacing=10, row_spacing=6)
        self.pc_rows = {}
        rows = [("cpu", "Processeur"), ("memory", "Mémoire"), ("disk", "Disque")]
        if battery() is not None:
            rows.insert(2, ("battery", "Batterie"))
        for row, (key, title) in enumerate(rows):
            box.attach(label(_t(title)), 0, row, 1, 1)
            bar = Gtk.LevelBar(min_value=0, max_value=100, hexpand=True, valign=Gtk.Align.CENTER)
            bar.get_accessible().set_name(_t(title))
            box.attach(bar, 1, row, 1, 1)
            value = label("", "dim", xalign=1, width_chars=14)
            box.attach(value, 2, row, 1, 1)
            self.pc_rows[key] = (bar, value)
        return box

    def update_pc(self):
        rows = getattr(self, "pc_rows", {})
        if "pc" not in self.cards or not rows:
            return
        current = cpu_times()
        percent = cpu_percent(self.cpu_previous, current)
        self.cpu_previous = current
        values = {}
        if percent is not None:
            values["cpu"] = (percent, f"{percent} %")
        used = memory()
        if used:
            values["memory"] = (100 * used[0] / used[1], _t("{used} sur {total}").format(
                used=size_text(used[0]), total=size_text(used[1])))
        level = battery()
        if level:
            values["battery"] = (level[0], f"{level[0]} %" + (f" · {_t('en charge')}" if level[1] else ""))
        space = disk()
        if space:
            values["disk"] = (100 * space[0] / space[1], _t("{used} sur {total}").format(
                used=size_text(space[0]), total=size_text(space[1])))
        for key, (bar, text) in rows.items():
            if key in values:
                bar.set_value(values[key][0])
                text.set_text(values[key][1])

    # Pense-bête
    def build_notes(self):
        view = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
        view.get_style_context().add_class("note")
        view.set_size_request(-1, 110)
        view.get_accessible().set_name(_t("Pense-bête"))
        buffer = view.get_buffer()
        buffer.set_text(self.data.get("notes", ""))
        buffer.connect("changed", self.notes_changed)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        box.pack_start(view, False, False, 0)
        box.pack_start(label(_t("Écrivez ici, c’est enregistré tout seul."), "dim"), False, False, 0)
        return box

    def notes_changed(self, buffer):
        self.data["notes"] = buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), False)[:20000]
        if self.save_source:
            GLib.source_remove(self.save_source)
        self.save_source = GLib.timeout_add(600, self.save_now)

    def save_now(self):
        if self.save_source:
            GLib.source_remove(self.save_source)
            self.save_source = 0
        try:
            stored = load_config()
            stored["notes"] = self.data.get("notes", "")
            save_config(stored)
        except OSError:
            pass
        return False

    # Photos
    def build_photos(self):
        folder, files = photo_files(self.data)
        if not files:
            return None
        self.photos = files
        self.photo_index %= len(files)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.photo = Gtk.Image()
        events = Gtk.EventBox()
        events.add(self.photo)
        events.connect("button-press-event", lambda *_: self.open_path(self.photos[self.photo_index]) or True)
        box.pack_start(events, False, False, 0)
        line = Gtk.Box(spacing=6)
        line.pack_start(icon_button("go-previous-symbolic", _t("Photo précédente"), lambda: self.show_photo(-1)), False, False, 0)
        self.photo_name = label("", "dim", ellipsize=Pango.EllipsizeMode.MIDDLE)
        line.pack_start(self.photo_name, True, True, 0)
        line.pack_end(icon_button("folder-pictures-symbolic", _t("Ouvrir le dossier"), lambda: self.open_path(folder)), False, False, 0)
        line.pack_end(icon_button("go-next-symbolic", _t("Photo suivante"), lambda: self.show_photo(1)), False, False, 0)
        box.pack_start(line, False, False, 0)
        self.show_photo(0)
        return box

    def show_photo(self, step):
        if not getattr(self, "photos", None):
            return
        self.photo_index = (self.photo_index + step) % len(self.photos)
        self.photo_tick = 0
        path = self.photos[self.photo_index]
        try:
            pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path), self.WIDTH - 60, 220, True)
            self.photo.set_from_pixbuf(pixbuf)
        except GLib.Error:
            self.photo.set_from_icon_name("image-missing", Gtk.IconSize.DIALOG)
        self.photo_name.set_text(path.name)

    def open_path(self, path):
        self.hide()
        try:
            subprocess.Popen(["xdg-open", str(path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError:
            pass

    # Minuteur
    def build_timer(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.timer_label = label("", "big", xalign=0.5)
        box.pack_start(self.timer_label, False, False, 0)
        presets = Gtk.Box(spacing=6, homogeneous=True)
        for minutes in (1, 5, 10, 25):
            button = Gtk.Button(label=f"{minutes} min")
            button.connect("clicked", lambda *_, m=minutes: self.set_timer(m * 60))
            presets.pack_start(button, True, True, 0)
        box.pack_start(presets, False, False, 0)
        controls = Gtk.Box(spacing=6, homogeneous=True)
        self.timer_start = Gtk.Button(label=_t("Démarrer"))
        self.timer_start.get_style_context().add_class("accent")
        self.timer_start.connect("clicked", lambda *_: self.toggle_timer())
        controls.pack_start(self.timer_start, True, True, 0)
        reset = Gtk.Button(label=_t("Réinitialiser"))
        reset.connect("clicked", lambda *_: self.set_timer(self.timer_total))
        controls.pack_start(reset, True, True, 0)
        box.pack_start(controls, False, False, 0)
        return box

    def keep_running(self, running):
        """Le minuteur continue volet fermé : l'application reste en vie."""
        if running and not self.held:
            self.hold()
        elif not running and self.held:
            self.release()
        self.held = running

    def set_timer(self, seconds):
        self.timer_total = self.timer_left = seconds
        self.timer_end = None
        self.keep_running(False)
        self.update_timer()

    def toggle_timer(self):
        if self.timer_end:
            self.timer_left = max(0, self.timer_end - time.monotonic())
            self.timer_end = None
            self.keep_running(False)
        else:
            if self.timer_left <= 0:
                self.timer_left = self.timer_total
            self.timer_end = time.monotonic() + self.timer_left
            self.keep_running(True)
        self.update_timer()

    def update_timer(self):
        if self.timer_end:
            self.timer_left = max(0, self.timer_end - time.monotonic())
            if self.timer_left <= 0:
                self.timer_end = None
                self.keep_running(False)
                self.notify(_t("Minuteur terminé"), _t("Le temps de {value} est écoulé.").format(value=duration_text(self.timer_total)))
        if "timer" not in self.cards:
            return
        self.timer_label.set_text(duration_text(self.timer_left))
        self.timer_start.set_label(_t("Pause") if self.timer_end else
                                   _t("Reprendre") if 0 < self.timer_left < self.timer_total else _t("Démarrer"))

    def notify(self, title, body):
        try:
            bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
            bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications",
                          "org.freedesktop.Notifications", "Notify",
                          GLib.Variant("(susssasa{sv}i)", (_t("Widgets"), 0, "alarm-symbolic", title, body, [],
                                                           {"urgency": GLib.Variant("y", 2)}, 0)),
                          None, Gio.DBusCallFlags.NONE, 2000, None)
        except GLib.Error:
            pass
        sound = Path("/usr/share/winlux/sounds/notification.wav")
        if sound.is_file() and shutil.which("aplay"):
            subprocess.Popen(["aplay", "-q", str(sound)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    # Battement d'horloge : heure, état du PC, minuteur, diaporama
    def tick(self):
        self.update_timer()
        if not self.window or not self.window.get_visible():
            return True
        self.clock.set_text(clock_text())
        self.update_pc()
        self.photo_tick += 1
        if self.photo_tick >= 10 and "photos" in self.cards:
            self.show_photo(1)
        return True


if __name__ == "__main__":
    raise SystemExit(Widgets().run([]))
