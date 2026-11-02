#!/usr/bin/python3
"""Ambiance de l'écran : éclairage nocturne (couleurs plus chaudes le soir).

Comme Windows 11 (Paramètres > Système > Écran > Éclairage nocturne) :
- interrupteur « Activer maintenant », intensité de 0 à 100 ;
- planning : désactivé, du coucher au lever du soleil, ou heures choisies ;
- le coucher et le lever du soleil sont calculés sans service en ligne, à
  partir des coordonnées du fuseau horaire (zone1970.tab de tzdata) ;
- au début et à la fin d'une plage du planning, l'interrupteur suit le
  planning ; entre les deux, il garde le choix de l'utilisateur ;
- passage progressif d'une couleur à l'autre.

Les couleurs sont appliquées par les rampes gamma de chaque écran (RandR,
libXrandr). Un pilote sans rampe gamma (certaines VM) rend la fonction
indisponible : Paramètres et réglages rapides la masquent.

    winlux-ambiance status            état, une valeur par ligne (clé=valeur)
    winlux-ambiance set CLÉ VALEUR    NIGHTLIGHT on|off, SCHEDULE off|sun|manual,
                                      FROM HH:MM, TO HH:MM, STRENGTH 0-100
    winlux-ambiance toggle            active ou coupe l'éclairage nocturne
    winlux-ambiance apply             applique tout de suite l'état voulu
    winlux-ambiance reset             couleurs normales sur tous les écrans
    winlux-ambiance daemon            suit le planning (lancé avec la session)

Réglages : ~/.config/winlux/ambiance.conf. Tests : WINLUX_AMBIANCE_FAKE=FICHIER
remplace l'écran par un fichier (rampes écrites en clair), WINLUX_AMBIANCE_NOW
fixe l'heure (AAAA-MM-JJTHH:MM), WINLUX_ZONEINFO déplace /usr/share/zoneinfo.
"""
import ctypes
import ctypes.util
import datetime as dt
import math
import os
from pathlib import Path
import sys
import time

DEFAULTS = {"NIGHTLIGHT": "off", "SCHEDULE": "off", "FROM": "21:00", "TO": "07:00",
            "STRENGTH": "50", "STAMP": ""}
DAY_KELVIN = 6500
NIGHT_MIN_KELVIN = 2700   # intensité 100 : comparable au réglage le plus chaud de Windows
FALLBACK_SUN = ("20:00", "07:00")


# ── Réglages ────────────────────────────────────────────────────────────────

def config_path():
    base = os.environ.get("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return Path(base) / "winlux" / "ambiance.conf"


def load():
    values = dict(DEFAULTS)
    try:
        for line in config_path().read_text(encoding="utf-8").splitlines():
            if "=" in line and not line.lstrip().startswith("#"):
                key, value = line.split("=", 1)
                if key.strip() in DEFAULTS: values[key.strip()] = value.strip()
    except OSError:
        pass
    return values


def save(values):
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    text = "".join(f"{key}={values[key]}\n" for key in DEFAULTS)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(text, encoding="utf-8")
    os.replace(temporary, path)


def parse_clock(text):
    try:
        hours, minutes = text.strip().split(":")
        hours, minutes = int(hours), int(minutes)
    except ValueError:
        return None
    return hours * 60 + minutes if 0 <= hours < 24 and 0 <= minutes < 60 else None


def validate(key, value):
    """Valeur normalisée, ou None si elle est refusée."""
    value = value.strip()
    if key == "NIGHTLIGHT": return value if value in ("on", "off") else None
    if key == "SCHEDULE": return value if value in ("off", "sun", "manual") else None
    if key in ("FROM", "TO"):
        minutes = parse_clock(value)
        return None if minutes is None else f"{minutes // 60:02d}:{minutes % 60:02d}"
    if key == "STRENGTH":
        try: number = int(value)
        except ValueError: return None
        return str(min(100, max(0, number)))
    return None


def now():
    fixed = os.environ.get("WINLUX_AMBIANCE_NOW")
    if fixed:
        try: return dt.datetime.fromisoformat(fixed)
        except ValueError: pass
    return dt.datetime.now()


# ── Soleil : coordonnées du fuseau horaire, puis équation du lever ─────────

def timezone_name():
    if os.environ.get("TZ", "").strip(": "):
        return os.environ["TZ"].strip(": ")
    try:
        name = Path("/etc/timezone").read_text(encoding="utf-8").strip()
        if name: return name
    except OSError:
        pass
    try:
        target = os.readlink("/etc/localtime")
        if "zoneinfo/" in target: return target.split("zoneinfo/", 1)[1]
    except OSError:
        pass
    return ""


def _coordinate(text, degrees_digits):
    sign = -1 if text[0] == "-" else 1
    digits = text[1:]
    degrees = int(digits[:degrees_digits])
    minutes = int(digits[degrees_digits:degrees_digits + 2])
    seconds = int(digits[degrees_digits + 2:] or 0)
    return sign * (degrees + minutes / 60 + seconds / 3600)


def zone_coordinates(name):
    """(latitude, longitude) du fuseau, d'après zone1970.tab ou zone.tab."""
    if not name: return None
    folder = Path(os.environ.get("WINLUX_ZONEINFO", "/usr/share/zoneinfo"))
    for table in ("zone1970.tab", "zone.tab"):
        try:
            lines = (folder / table).read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        for line in lines:
            if line.startswith("#"): continue
            fields = line.split("\t")
            if len(fields) < 3 or fields[2] != name: continue
            position = fields[1]
            split = max(position.rfind("+"), position.rfind("-"))
            if split <= 0: continue
            try:
                latitude = _coordinate(position[:split], 2)
                longitude = _coordinate(position[split:], 3)
            except ValueError:
                continue
            return latitude, longitude
    return None


def sun_times(day, latitude, longitude, offset_hours):
    """Lever et coucher (minutes depuis minuit, heure locale) ; None l'été polaire ou l'hiver polaire."""
    n = day.timetuple().tm_yday
    gamma = 2 * math.pi / 365 * (n - 1)
    equation = 229.18 * (0.000075 + 0.001868 * math.cos(gamma) - 0.032077 * math.sin(gamma)
                         - 0.014615 * math.cos(2 * gamma) - 0.040849 * math.sin(2 * gamma))
    declination = (0.006918 - 0.399912 * math.cos(gamma) + 0.070257 * math.sin(gamma)
                   - 0.006758 * math.cos(2 * gamma) + 0.000907 * math.sin(2 * gamma)
                   - 0.002697 * math.cos(3 * gamma) + 0.00148 * math.sin(3 * gamma))
    phi = math.radians(latitude)
    cos_h = (math.cos(math.radians(90.833)) / (math.cos(phi) * math.cos(declination))
             - math.tan(phi) * math.tan(declination))
    if cos_h < -1 or cos_h > 1: return None
    hour_angle = math.degrees(math.acos(cos_h))
    noon = 720 - 4 * longitude - equation + offset_hours * 60
    return (round(noon - 4 * hour_angle) % 1440, round(noon + 4 * hour_angle) % 1440)


def sun_window(moment):
    """(coucher, lever) en « HH:MM », et True s'ils viennent du calcul réel."""
    coordinates = zone_coordinates(timezone_name())
    if coordinates:
        offset = moment.astimezone().utcoffset() if moment.tzinfo is None else moment.utcoffset()
        offset_hours = offset.total_seconds() / 3600 if offset is not None else 0
        result = sun_times(moment.date(), coordinates[0], coordinates[1], offset_hours)
        if result:
            sunrise, sunset = result
            return f"{sunset // 60:02d}:{sunset % 60:02d}", f"{sunrise // 60:02d}:{sunrise % 60:02d}", True
    return FALLBACK_SUN[0], FALLBACK_SUN[1], False


# ── Planning ────────────────────────────────────────────────────────────────

def schedule_window(values, moment):
    """(début, fin) de la plage de nuit en minutes, ou None sans planning."""
    if values["SCHEDULE"] == "sun":
        start, end, _ = sun_window(moment)
    elif values["SCHEDULE"] == "manual":
        start, end = values["FROM"], values["TO"]
    else:
        return None
    start, end = parse_clock(start), parse_clock(end)
    if start is None or end is None or start == end: return None
    return start, end


def scheduled_period(values, moment):
    """(nuit ?, repère de la plage en cours) ; repère vide sans planning."""
    window = schedule_window(values, moment)
    if window is None: return False, ""
    start, end = window
    minute = moment.hour * 60 + moment.minute
    day = moment.date()
    if start < end:
        night = start <= minute < end
        if night: return True, f"{day}N"
        return False, f"{day if minute >= end else day - dt.timedelta(days=1)}D"
    # Plage à cheval sur minuit (21:00 → 07:00).
    if minute >= start: return True, f"{day}N"
    if minute < end: return True, f"{day - dt.timedelta(days=1)}N"
    return False, f"{day}D"


def follow_schedule(values, moment):
    """Au changement de plage, l'interrupteur reprend la valeur du planning."""
    night, stamp = scheduled_period(values, moment)
    if stamp and stamp != values.get("STAMP"):
        values["STAMP"] = stamp
        values["NIGHTLIGHT"] = "on" if night else "off"
        return True
    return False


def target_kelvin(values):
    if values["NIGHTLIGHT"] != "on": return DAY_KELVIN
    try: strength = min(100, max(0, int(values["STRENGTH"])))
    except ValueError: strength = 50
    return round(DAY_KELVIN - (DAY_KELVIN - NIGHT_MIN_KELVIN) * strength / 100)


def kelvin_rgb(kelvin):
    """Multiplicateurs rouge, vert, bleu (0..1) pour une température de couleur."""
    kelvin = min(DAY_KELVIN, max(1000, kelvin))
    if kelvin >= DAY_KELVIN: return 1.0, 1.0, 1.0
    t = kelvin / 100
    green = (99.4708025861 * math.log(t) - 161.1195681661) / 255
    blue = 0.0 if t <= 19 else (138.5177312231 * math.log(t - 10) - 305.0447927307) / 255
    # Normalisé sur 6500 K (blanc neutre) : le rouge reste plein.
    reference_green = (99.4708025861 * math.log(65) - 161.1195681661) / 255
    reference_blue = (138.5177312231 * math.log(55) - 305.0447927307) / 255
    clamp = lambda value: min(1.0, max(0.0, value))
    return 1.0, clamp(green / reference_green), clamp(blue / reference_blue)


# ── Écrans : rampes gamma RandR ─────────────────────────────────────────────

class _Resources(ctypes.Structure):
    _fields_ = [("timestamp", ctypes.c_ulong), ("configTimestamp", ctypes.c_ulong),
                ("ncrtc", ctypes.c_int), ("crtcs", ctypes.POINTER(ctypes.c_ulong)),
                ("noutput", ctypes.c_int), ("outputs", ctypes.POINTER(ctypes.c_ulong)),
                ("nmode", ctypes.c_int), ("modes", ctypes.c_void_p)]


class _Gamma(ctypes.Structure):
    _fields_ = [("size", ctypes.c_int), ("red", ctypes.POINTER(ctypes.c_ushort)),
                ("green", ctypes.POINTER(ctypes.c_ushort)), ("blue", ctypes.POINTER(ctypes.c_ushort))]


class RandrScreens:
    """Rampes gamma de tous les CRTC de l'affichage X courant."""

    def __init__(self):
        x11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
        xrandr = ctypes.CDLL(ctypes.util.find_library("Xrandr") or "libXrandr.so.2")
        x11.XOpenDisplay.restype = ctypes.c_void_p
        x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x11.XDefaultRootWindow.restype = ctypes.c_ulong
        x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
        xrandr.XRRGetScreenResourcesCurrent.restype = ctypes.POINTER(_Resources)
        xrandr.XRRGetScreenResourcesCurrent.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        xrandr.XRRFreeScreenResources.argtypes = [ctypes.POINTER(_Resources)]
        xrandr.XRRGetCrtcGammaSize.restype = ctypes.c_int
        xrandr.XRRGetCrtcGammaSize.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        xrandr.XRRAllocGamma.restype = ctypes.POINTER(_Gamma)
        xrandr.XRRAllocGamma.argtypes = [ctypes.c_int]
        xrandr.XRRSetCrtcGamma.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(_Gamma)]
        xrandr.XRRFreeGamma.argtypes = [ctypes.POINTER(_Gamma)]
        self.x11, self.xrandr = x11, xrandr
        self.display = x11.XOpenDisplay(None)
        if not self.display: raise OSError("affichage X indisponible")

    def crtcs(self):
        resources = self.xrandr.XRRGetScreenResourcesCurrent(self.display, self.x11.XDefaultRootWindow(self.display))
        if not resources: return []
        try:
            found = []
            for index in range(resources.contents.ncrtc):
                crtc = resources.contents.crtcs[index]
                size = self.xrandr.XRRGetCrtcGammaSize(self.display, crtc)
                if size > 1: found.append((crtc, size))
            return found
        finally:
            self.xrandr.XRRFreeScreenResources(resources)

    def supported(self):
        return bool(self.crtcs())

    def apply(self, rgb):
        for crtc, size in self.crtcs():
            gamma = self.xrandr.XRRAllocGamma(size)
            if not gamma: continue
            try:
                for index in range(size):
                    level = index / (size - 1)
                    gamma.contents.red[index] = int(65535 * level * rgb[0])
                    gamma.contents.green[index] = int(65535 * level * rgb[1])
                    gamma.contents.blue[index] = int(65535 * level * rgb[2])
                self.xrandr.XRRSetCrtcGamma(self.display, crtc, gamma)
            finally:
                self.xrandr.XRRFreeGamma(gamma)
        # Attendre que le serveur ait tout reçu : la commande peut se terminer aussitôt après.
        self.x11.XSync(self.display, 0)


class FakeScreens:
    """Écran simulé pour les tests : la dernière couleur est écrite dans un fichier."""

    def __init__(self, path):
        self.path = Path(path)

    def supported(self):
        return os.environ.get("WINLUX_AMBIANCE_FAKE_SUPPORTED", "1") == "1"

    def apply(self, rgb):
        self.path.write_text("%.4f %.4f %.4f\n" % rgb, encoding="ascii")


def screens():
    fake = os.environ.get("WINLUX_AMBIANCE_FAKE")
    if fake: return FakeScreens(fake)
    try: return RandrScreens()
    except OSError:
        return None


# ── Commandes ───────────────────────────────────────────────────────────────

def status_lines(values, moment, output):
    sunset, sunrise, real = sun_window(moment)
    supported = output is not None and output.supported()
    return [f"supported={int(supported)}", f"nightlight={values['NIGHTLIGHT']}",
            f"schedule={values['SCHEDULE']}", f"from={values['FROM']}", f"to={values['TO']}",
            f"strength={values['STRENGTH']}", f"sunset={sunset}", f"sunrise={sunrise}",
            f"sun_known={int(real)}", f"kelvin={target_kelvin(values)}"]


def apply_now(values, output, previous=None, steps=1):
    """Applique la couleur voulue, en fondu depuis `previous` (kelvins)."""
    if output is None or not output.supported(): return None
    target = target_kelvin(values)
    start = previous if previous is not None else target
    for step in range(1, max(1, steps) + 1):
        kelvin = start + (target - start) * step / max(1, steps)
        output.apply(kelvin_rgb(kelvin))
        if step < steps: time.sleep(0.05)
    return target


def daemon():
    output = screens()
    current = None
    last_full = 0.0
    while True:
        values = load()
        if follow_schedule(values, now()): save(values)
        if output is None:
            output = screens()
        target = target_kelvin(values)
        # Rampe réécrite quand la couleur change, et chaque minute la nuit (écran
        # rebranché). De jour, sans changement, les rampes de l'écran ne sont pas touchées.
        if current is None and target == DAY_KELVIN:
            current = target
        elif output is not None and (target != current or (target != DAY_KELVIN and time.monotonic() - last_full > 60)):
            try:
                current = apply_now(values, output, current, steps=20 if current not in (None, target) else 1)
            except OSError:
                output = None
            last_full = time.monotonic()
        time.sleep(5)


def main(argv):
    command = argv[1] if len(argv) > 1 else "status"
    values = load()
    moment = now()
    if command == "status":
        print("\n".join(status_lines(values, moment, screens())))
        return 0
    if command == "set" and len(argv) == 4:
        key = argv[2].upper()
        value = validate(key, argv[3]) if key in DEFAULTS and key != "STAMP" else None
        if value is None:
            print(f"winlux-ambiance : valeur refusée pour {argv[2]}", file=sys.stderr)
            return 2
        values[key] = value
        if key in ("SCHEDULE", "FROM", "TO"):
            # Nouveau planning : il s'applique tout de suite.
            values["STAMP"] = ""
            follow_schedule(values, moment)
        save(values)
        apply_now(values, screens(), steps=1)
        return 0
    if command == "toggle":
        follow_schedule(values, moment)
        values["NIGHTLIGHT"] = "off" if values["NIGHTLIGHT"] == "on" else "on"
        save(values)
        apply_now(values, screens(), steps=1)
        return 0
    if command == "apply":
        if follow_schedule(values, moment): save(values)
        return 0 if apply_now(values, screens()) is not None else 1
    if command == "reset":
        output = screens()
        if output is not None and output.supported(): output.apply((1.0, 1.0, 1.0))
        return 0
    if command == "daemon":
        try: daemon()
        except KeyboardInterrupt: pass
        return 0
    print(__doc__.strip().split("\n\n")[-2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
