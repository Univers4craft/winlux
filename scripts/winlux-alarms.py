#!/usr/bin/python3
"""Horloge : alarmes, minuteurs, chronomètre, horloges du monde et concentration.

Horloge Winlux. Les alarmes, les minuteurs et la
fin d'une session de concentration sonnent même quand la fenêtre est fermée
(et même écran verrouillé) : un petit processus de la session s'en charge.
Une session de concentration active « Ne pas déranger » pendant sa durée.

    winlux-alarms                    ouvre l'Horloge
    winlux-alarms daemon             surveille alarmes et minuteurs (avec la session)
    winlux-alarms add-alarm HH:MM [JOURS] [NOM]   JOURS : 0123456 (lundi = 0) ou « - »
    winlux-alarms add-timer SECONDES [NOM]        démarre un minuteur
    winlux-alarms focus MINUTES                   démarre une session de concentration
    winlux-alarms stop-focus
    winlux-alarms list               état (tests)
    WINLUX_CLOCK_PAGE=focus|timer|alarm|stopwatch|world ouvre directement une page.

État : ~/.local/state/winlux/clock.json.
"""
import datetime as dt
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid


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
    'Horloge': ('Clock', 'Reloj', 'Uhr'),
    'Concentration': ('Focus sessions', 'Sesiones de concentración', 'Fokussitzungen'),
    'Minuteur': ('Timer', 'Temporizador', 'Timer'),
    'Alarme': ('Alarm', 'Alarma', 'Wecker'),
    'Chronomètre': ('Stopwatch', 'Cronómetro', 'Stoppuhr'),
    'Horloge mondiale': ('World clock', 'Reloj mundial', 'Weltuhr'),
    'Démarrer': ('Start', 'Iniciar', 'Starten'),
    'Pause': ('Pause', 'Pausa', 'Pause'),
    'Reprendre': ('Resume', 'Reanudar', 'Fortsetzen'),
    'Réinitialiser': ('Reset', 'Restablecer', 'Zurücksetzen'),
    'Arrêter': ('Stop', 'Detener', 'Beenden'),
    'Supprimer': ('Delete', 'Eliminar', 'Löschen'),
    'Ajouter': ('Add', 'Agregar', 'Hinzufügen'),
    'Tour': ('Lap', 'Vuelta', 'Runde'),
    'Tour {n}': ('Lap {n}', 'Vuelta {n}', 'Runde {n}'),
    'Nom': ('Name', 'Nombre', 'Name'),
    'Heure': ('Time', 'Hora', 'Uhrzeit'),
    'Répéter': ('Repeat', 'Repetir', 'Wiederholen'),
    'Une seule fois': ('Only once', 'Solo una vez', 'Nur einmal'),
    'Tous les jours': ('Every day', 'Todos los días', 'Täglich'),
    'Aucune alarme.': ('No alarms.', 'No hay alarmas.', 'Keine Wecker.'),
    'Aucun minuteur en cours.': ('No running timers.', 'No hay temporizadores.', 'Keine laufenden Timer.'),
    'Durée (minutes)': ('Duration (minutes)', 'Duración (minutos)', 'Dauer (Minuten)'),
    'Ne pas déranger pendant la session.': ('Do not disturb during the session.', 'No molestar durante la sesión.', 'Nicht stören während der Sitzung.'),
    'Session en cours : {left} restantes': ('Session running: {left} left', 'Sesión en curso: quedan {left}', 'Sitzung läuft: noch {left}'),
    'Session de concentration terminée': ('Focus session complete', 'Sesión de concentración terminada', 'Fokussitzung beendet'),
    'Minuteur terminé': ('Timer done', 'Temporizador terminado', 'Timer abgelaufen'),
    'Rappel dans 5 min': ('Snooze 5 min', 'Posponer 5 min', 'In 5 Min. erinnern'),
    'Ignorer': ('Dismiss', 'Descartar', 'Schließen'),
    'Rechercher une ville ou un fuseau': ('Search a city or time zone', 'Buscar una ciudad o zona horaria', 'Stadt oder Zeitzone suchen'),
    'Heure locale': ('Local time', 'Hora local', 'Ortszeit'),
    '{h} h d’avance': ('{h} h ahead', '{h} h por delante', '{h} Std. voraus'),
    '{h} h de retard': ('{h} h behind', '{h} h por detrás', '{h} Std. zurück'),
    'Même heure': ('Same time', 'Misma hora', 'Gleiche Zeit'),
    'Prochaine : {when}': ('Next: {when}', 'Próxima: {when}', 'Nächster: {when}'),
    'Lun': ('Mon', 'Lun', 'Mo'), 'Mar': ('Tue', 'Mar', 'Di'), 'Mer': ('Wed', 'Mié', 'Mi'), 'Jeu': ('Thu', 'Jue', 'Do'),
    'Ven': ('Fri', 'Vie', 'Fr'), 'Sam': ('Sat', 'Sáb', 'Sa'), 'Dim': ('Sun', 'Dom', 'So'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


DAYS = ("Lun", "Mar", "Mer", "Jeu", "Ven", "Sam", "Dim")
SNOOZE = 5 * 60
RING_SECONDS = 5 * 60


def state_path():
    return Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state") / "winlux" / "clock.json"


def dnd_path():
    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "winlux" / "do-not-disturb"


def load():
    try:
        data = json.loads(state_path().read_text(encoding="utf-8"))
    except (OSError, ValueError):
        data = {}
    data.setdefault("alarms", [])
    data.setdefault("timers", [])
    data.setdefault("zones", [])
    data.setdefault("focus", {})
    return data


def save(data):
    path = state_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=1), encoding="utf-8")
    os.replace(temporary, path)


def now():
    fixed = os.environ.get("WINLUX_CLOCK_NOW")
    if fixed:
        try: return dt.datetime.fromisoformat(fixed)
        except ValueError: pass
    return dt.datetime.now()


# ── Logique (testée) ────────────────────────────────────────────────────────

def parse_clock(text):
    try:
        hours, minutes = (int(part) for part in text.strip().split(":"))
    except ValueError:
        return None
    return (hours, minutes) if 0 <= hours < 24 and 0 <= minutes < 60 else None


def new_alarm(clock, days=(), label=""):
    hours, minutes = parse_clock(clock)
    return {"id": uuid.uuid4().hex[:8], "time": f"{hours:02d}:{minutes:02d}", "days": sorted(set(days)),
            "label": label, "on": True, "last": "", "snooze": 0}


def next_ring(alarm, moment):
    """Prochaine sonnerie de l'alarme (datetime), ou None si elle est coupée."""
    if not alarm.get("on"): return None
    if alarm.get("snooze", 0) > moment.timestamp():
        return dt.datetime.fromtimestamp(alarm["snooze"])
    hours, minutes = parse_clock(alarm["time"])
    for offset in range(8):
        day = moment.date() + dt.timedelta(days=offset)
        candidate = dt.datetime.combine(day, dt.time(hours, minutes))
        if candidate < moment.replace(second=0, microsecond=0): continue
        if alarm["days"] and day.weekday() not in alarm["days"]: continue
        stamp = candidate.strftime("%Y-%m-%dT%H:%M")
        if stamp == alarm.get("last"): continue
        return candidate
    return None


def due_alarms(data, moment):
    """Alarmes à faire sonner maintenant ; elles sont marquées (une sonnerie par minute prévue)."""
    ringing = []
    minute = moment.replace(second=0, microsecond=0)
    for alarm in data["alarms"]:
        if not alarm.get("on"): continue
        if alarm.get("snooze") and alarm["snooze"] <= moment.timestamp():
            alarm["snooze"] = 0
            ringing.append(alarm)
            continue
        hours, minutes = parse_clock(alarm["time"])
        planned = minute.replace(hour=hours, minute=minutes)
        stamp = planned.strftime("%Y-%m-%dT%H:%M")
        if planned != minute or stamp == alarm.get("last"): continue
        if alarm["days"] and minute.weekday() not in alarm["days"]: continue
        alarm["last"] = stamp
        if not alarm["days"]: alarm["on"] = False   # alarme « une seule fois »
        ringing.append(alarm)
    return ringing


def new_timer(seconds, label="", start=None):
    seconds = max(1, int(seconds))
    return {"id": uuid.uuid4().hex[:8], "label": label, "seconds": seconds,
            "remaining": seconds, "end": (start + seconds) if start is not None else None}


def timer_left(timer, stamp):
    if timer.get("end") is None: return timer["remaining"]
    return max(0, round(timer["end"] - stamp))


def due_timers(data, stamp):
    finished = []
    for timer in data["timers"]:
        if timer.get("end") is not None and timer["end"] <= stamp:
            timer["end"] = None
            timer["remaining"] = timer["seconds"]
            finished.append(timer)
    return finished


def focus_start(data, minutes, stamp):
    minutes = min(240, max(5, int(minutes)))
    dnd = dnd_path()
    created = not dnd.exists()
    if created:
        dnd.parent.mkdir(parents=True, exist_ok=True)
        dnd.touch(mode=0o600)
    data["focus"] = {"end": stamp + minutes * 60, "minutes": minutes, "dnd": created}
    return data


def focus_stop(data):
    if data["focus"].get("dnd"):
        try: dnd_path().unlink()
        except OSError: pass
    data["focus"] = {}
    return data


def focus_due(data, stamp):
    end = data["focus"].get("end")
    if end and end <= stamp:
        focus_stop(data)
        return True
    return False


def duration(seconds):
    seconds = max(0, int(round(seconds)))
    hours, rest = divmod(seconds, 3600)
    minutes, seconds = divmod(rest, 60)
    return f"{hours:02d}:{minutes:02d}:{seconds:02d}"


def offset_text(zone_offset, local_offset):
    hours = (zone_offset - local_offset) / 3600
    if abs(hours) < 0.01: return _t("Même heure")
    shown = f"{abs(hours):g}"
    return _t("{h} h d’avance").format(h=shown) if hours > 0 else _t("{h} h de retard").format(h=shown)


def zone_names():
    folder = Path(os.environ.get("WINLUX_ZONEINFO", "/usr/share/zoneinfo"))
    names = []
    for table in ("zone1970.tab", "zone.tab"):
        try:
            for line in (folder / table).read_text(encoding="utf-8").splitlines():
                if not line.startswith("#") and line.count("\t") >= 2:
                    names.append(line.split("\t")[2])
        except OSError:
            continue
        if names: break
    return sorted(set(names))


def zone_time(name, moment_utc):
    try:
        from zoneinfo import ZoneInfo
        return moment_utc.astimezone(ZoneInfo(name))
    except Exception:
        return None


# ── Sonnerie ────────────────────────────────────────────────────────────────

def sound_file():
    for path in (Path("/usr/share/winlux/sounds/alarme.wav"),
                 Path(__file__).resolve().parent.parent / "assets/sounds/alarme.wav"):
        if path.is_file(): return path
    return None


def play_once():
    path = sound_file()
    player = shutil.which("paplay") or shutil.which("aplay")
    if path is None or player is None: return None
    try:
        return subprocess.Popen([player, str(path)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except OSError:
        return None


def run_daemon():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GLib, Gtk

    rings = []
    css = Gtk.CssProvider()
    css.load_from_data(b"""
      window { background: #202430; color: #ecedf5; }
      .heading { font-size: 20px; font-weight: bold; }
      button { background: #30384b; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 8px 14px; box-shadow: none; }
    """)
    from gi.repository import Gdk
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), css, 600)

    def ring(title, text, alarm_id=None):
        window = Gtk.Window(title=title)
        window.set_role("winlux-alarm")
        window.set_keep_above(True)
        window.set_urgency_hint(True)
        window.set_default_size(340, -1)
        window.set_position(Gtk.WindowPosition.CENTER)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10, margin=18)
        window.add(box)
        heading = Gtk.Label(label=title, xalign=0)
        heading.get_style_context().add_class("heading")
        box.pack_start(heading, False, False, 0)
        if text: box.pack_start(Gtk.Label(label=text, xalign=0, wrap=True), False, False, 0)
        buttons = Gtk.Box(spacing=8)
        box.pack_start(buttons, False, False, 0)
        started = time.monotonic()
        state = {"player": play_once()}

        def stop():
            if state["player"] is not None and state["player"].poll() is None: state["player"].terminate()
            state["player"] = "stopped"
            window.destroy()
            if window in rings: rings.remove(window)

        def snooze():
            data = load()
            for alarm in data["alarms"]:
                if alarm["id"] == alarm_id:
                    alarm["snooze"] = time.time() + SNOOZE
                    alarm["on"] = True
            save(data)
            stop()

        if alarm_id:
            again = Gtk.Button(label=_t("Rappel dans 5 min"))
            again.connect("clicked", lambda *_: snooze())
            buttons.pack_start(again, True, True, 0)
        close = Gtk.Button(label=_t("Ignorer"))
        close.connect("clicked", lambda *_: stop())
        buttons.pack_start(close, True, True, 0)
        window.connect("delete-event", lambda *_: stop() or True)

        def repeat():
            if state["player"] == "stopped": return False
            if time.monotonic() - started > RING_SECONDS:
                stop()
                return False
            if state["player"] is None or state["player"].poll() is not None: state["player"] = play_once()
            return True
        GLib.timeout_add(500, repeat)
        rings.append(window)
        window.show_all()
        window.present()

    def tick():
        data = load()
        moment = now()
        stamp = time.time()
        changed = False
        for alarm in due_alarms(data, moment):
            changed = True
            ring(alarm.get("label") or _t("Alarme"), alarm["time"], alarm["id"])
        for timer in due_timers(data, stamp):
            changed = True
            ring(_t("Minuteur terminé"), timer.get("label") or duration(timer["seconds"]))
        if focus_due(data, stamp):
            changed = True
            ring(_t("Session de concentration terminée"), "")
        if changed: save(data)
        return True

    tick()
    GLib.timeout_add(2000, tick)
    Gtk.main()
    return 0


# ── Application (GTK) ───────────────────────────────────────────────────────

def run_app():
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gio, GLib, Gtk

    class Clock(Gtk.ApplicationWindow):
        def __init__(self, app):
            super().__init__(application=app, title=_t("Horloge"))
            self.set_icon_name("winlux-alarms")
            self.set_default_size(760, 560)
            css = Gtk.CssProvider()
            css.load_from_data(b"""
              window { background: #181b24; color: #ecedf5; }
              stacksidebar, stacksidebar list { background: #202430; }
              stacksidebar row { padding: 10px 14px; color: #ecedf5; }
              stacksidebar row label { color: #ecedf5; }
              stacksidebar row:selected { background: #423d66; }
              .big { font-size: 56px; font-weight: 300; }
              .mid { font-size: 28px; }
              .card { background: #242a38; border-radius: 8px; padding: 14px; }
              .subtitle { color: #adb6c9; }
              button { background: #30384b; color: #ecedf5; border: 1px solid #48536b; border-radius: 6px; padding: 8px 14px; box-shadow: none; }
              button.accent { background: #6a3df0; border-color: #7c4dff; }
              entry, spinbutton { background: #171b25; color: #ecedf5; }
            """)
            Gtk.StyleContext.add_provider_for_screen(self.get_screen(), css, 600)
            outer = Gtk.Box()
            self.add(outer)
            self.stack = Gtk.Stack(transition_type=Gtk.StackTransitionType.CROSSFADE)
            sidebar = Gtk.StackSidebar(stack=self.stack)
            sidebar.set_size_request(200, -1)
            outer.pack_start(sidebar, False, False, 0)
            outer.pack_start(self.stack, True, True, 0)
            self.pages = {}
            for key, title in (("focus", _t("Concentration")), ("timer", _t("Minuteur")), ("alarm", _t("Alarme")),
                               ("stopwatch", _t("Chronomètre")), ("world", _t("Horloge mondiale"))):
                scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
                box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14, margin=24)
                scroll.add(box)
                self.stack.add_titled(scroll, key, title)
                self.pages[key] = box
            self.watch = {"start": None, "elapsed": 0.0, "laps": []}
            self.build_all()
            GLib.timeout_add(250, self.tick)
            self.show_all()
            page = os.environ.get("WINLUX_CLOCK_PAGE")
            if page in self.pages: self.stack.set_visible_child_name(page)

        def clear(self, key):
            for child in self.pages[key].get_children(): child.destroy()
            return self.pages[key]

        def card(self, parent):
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
            box.get_style_context().add_class("card")
            parent.pack_start(box, False, False, 0)
            return box

        def build_all(self):
            for build in (self.build_focus, self.build_timer, self.build_alarm, self.build_stopwatch, self.build_world):
                build()
            for page in self.pages.values(): page.show_all()

        # Concentration
        def build_focus(self):
            page = self.clear("focus")
            data = load()
            self.focus_label = Gtk.Label(xalign=0.5)
            self.focus_label.get_style_context().add_class("big")
            page.pack_start(self.focus_label, False, False, 0)
            if data["focus"].get("end"):
                stop = Gtk.Button(label=_t("Arrêter"), halign=Gtk.Align.CENTER)
                stop.connect("clicked", lambda *_: (save(focus_stop(load())), self.build_focus(), page.show_all()))
                page.pack_start(stop, False, False, 0)
            else:
                row = Gtk.Box(spacing=10, halign=Gtk.Align.CENTER)
                row.pack_start(Gtk.Label(label=_t("Durée (minutes)")), False, False, 0)
                self.focus_minutes = Gtk.SpinButton.new_with_range(5, 240, 5)
                self.focus_minutes.set_value(30)
                row.pack_start(self.focus_minutes, False, False, 0)
                page.pack_start(row, False, False, 0)
                start = Gtk.Button(label=_t("Démarrer"), halign=Gtk.Align.CENTER)
                start.get_style_context().add_class("accent")
                start.connect("clicked", lambda *_: (save(focus_start(load(), self.focus_minutes.get_value(), time.time())),
                                                    self.build_focus(), page.show_all()))
                page.pack_start(start, False, False, 0)
            hint = Gtk.Label(label=_t("Ne pas déranger pendant la session."), xalign=0.5)
            hint.get_style_context().add_class("subtitle")
            page.pack_start(hint, False, False, 0)

        # Minuteurs
        def build_timer(self):
            page = self.clear("timer")
            data = load()
            add = self.card(page)
            row = Gtk.Box(spacing=8)
            self.timer_h = Gtk.SpinButton.new_with_range(0, 23, 1)
            self.timer_m = Gtk.SpinButton.new_with_range(0, 59, 1)
            self.timer_s = Gtk.SpinButton.new_with_range(0, 59, 1)
            self.timer_m.set_value(5)
            for spin, unit in ((self.timer_h, "h"), (self.timer_m, "min"), (self.timer_s, "s")):
                row.pack_start(spin, False, False, 0)
                row.pack_start(Gtk.Label(label=unit), False, False, 0)
            self.timer_name = Gtk.Entry(placeholder_text=_t("Nom"))
            row.pack_start(self.timer_name, True, True, 0)
            button = Gtk.Button(label=_t("Démarrer"))
            button.get_style_context().add_class("accent")
            button.connect("clicked", lambda *_: self.add_timer())
            row.pack_start(button, False, False, 0)
            add.pack_start(row, False, False, 0)
            self.timer_labels = {}
            if not data["timers"]:
                page.pack_start(Gtk.Label(label=_t("Aucun minuteur en cours."), xalign=0), False, False, 0)
            for timer in data["timers"]:
                box = self.card(page)
                if timer.get("label"): box.pack_start(Gtk.Label(label=timer["label"], xalign=0), False, False, 0)
                label = Gtk.Label(xalign=0)
                label.get_style_context().add_class("mid")
                box.pack_start(label, False, False, 0)
                self.timer_labels[timer["id"]] = label
                actions = Gtk.Box(spacing=8)
                running = timer.get("end") is not None
                toggle = Gtk.Button(label=_t("Pause") if running else (_t("Reprendre") if timer["remaining"] < timer["seconds"] else _t("Démarrer")))
                toggle.connect("clicked", lambda *_, i=timer["id"]: self.toggle_timer(i))
                reset = Gtk.Button(label=_t("Réinitialiser"))
                reset.connect("clicked", lambda *_, i=timer["id"]: self.edit_timer(i, reset=True))
                delete = Gtk.Button(label=_t("Supprimer"))
                delete.connect("clicked", lambda *_, i=timer["id"]: self.edit_timer(i, delete=True))
                for widget in (toggle, reset, delete): actions.pack_start(widget, False, False, 0)
                box.pack_start(actions, False, False, 0)

        def add_timer(self):
            seconds = int(self.timer_h.get_value() * 3600 + self.timer_m.get_value() * 60 + self.timer_s.get_value())
            if seconds <= 0: return
            data = load()
            data["timers"].append(new_timer(seconds, self.timer_name.get_text().strip(), time.time()))
            save(data)
            self.build_timer()
            self.pages["timer"].show_all()

        def toggle_timer(self, ident):
            data = load()
            for timer in data["timers"]:
                if timer["id"] != ident: continue
                if timer.get("end") is None: timer["end"] = time.time() + timer["remaining"]
                else:
                    timer["remaining"] = timer_left(timer, time.time())
                    timer["end"] = None
            save(data)
            self.build_timer()
            self.pages["timer"].show_all()

        def edit_timer(self, ident, reset=False, delete=False):
            data = load()
            if delete: data["timers"] = [t for t in data["timers"] if t["id"] != ident]
            for timer in data["timers"]:
                if timer["id"] == ident and reset:
                    timer["end"], timer["remaining"] = None, timer["seconds"]
            save(data)
            self.build_timer()
            self.pages["timer"].show_all()

        # Alarmes
        def build_alarm(self):
            page = self.clear("alarm")
            data = load()
            add = self.card(page)
            row = Gtk.Box(spacing=8)
            row.pack_start(Gtk.Label(label=_t("Heure")), False, False, 0)
            self.alarm_h = Gtk.SpinButton.new_with_range(0, 23, 1)
            self.alarm_m = Gtk.SpinButton.new_with_range(0, 59, 1)
            self.alarm_h.set_value(7)
            for spin in (self.alarm_h, self.alarm_m):
                spin.set_wrap(True)
                spin.connect("output", lambda s: s.set_text(f"{int(s.get_value()):02d}") or True)
            row.pack_start(self.alarm_h, False, False, 0)
            row.pack_start(Gtk.Label(label=":"), False, False, 0)
            row.pack_start(self.alarm_m, False, False, 0)
            self.alarm_name = Gtk.Entry(placeholder_text=_t("Nom"))
            row.pack_start(self.alarm_name, True, True, 0)
            add.pack_start(row, False, False, 0)
            days = Gtk.Box(spacing=4)
            days.pack_start(Gtk.Label(label=_t("Répéter")), False, False, 0)
            self.alarm_days = []
            for index, day in enumerate(DAYS):
                check = Gtk.ToggleButton(label=_t(day))
                days.pack_start(check, False, False, 0)
                self.alarm_days.append(check)
            button = Gtk.Button(label=_t("Ajouter"))
            button.get_style_context().add_class("accent")
            button.connect("clicked", lambda *_: self.add_alarm())
            days.pack_end(button, False, False, 0)
            add.pack_start(days, False, False, 0)
            if not data["alarms"]:
                page.pack_start(Gtk.Label(label=_t("Aucune alarme."), xalign=0), False, False, 0)
            moment = now()
            for alarm in sorted(data["alarms"], key=lambda a: a["time"]):
                box = self.card(page)
                head = Gtk.Box(spacing=8)
                clock = Gtk.Label(label=alarm["time"], xalign=0)
                clock.get_style_context().add_class("mid")
                head.pack_start(clock, False, False, 0)
                names = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
                names.pack_start(Gtk.Label(label=alarm.get("label") or _t("Alarme"), xalign=0), False, False, 0)
                repeat = (_t("Tous les jours") if len(alarm["days"]) == 7 else
                          " ".join(_t(DAYS[d]) for d in alarm["days"]) if alarm["days"] else _t("Une seule fois"))
                ring = next_ring(alarm, moment)
                if ring: repeat += " · " + _t("Prochaine : {when}").format(when=ring.strftime("%d/%m %H:%M"))
                info = Gtk.Label(label=repeat, xalign=0)
                info.get_style_context().add_class("subtitle")
                names.pack_start(info, False, False, 0)
                head.pack_start(names, True, True, 0)
                switch = Gtk.Switch(active=alarm.get("on", True), valign=Gtk.Align.CENTER)
                switch.connect("notify::active", lambda w, _, i=alarm["id"]: self.edit_alarm(i, on=w.get_active()))
                head.pack_start(switch, False, False, 0)
                delete = Gtk.Button(label=_t("Supprimer"), valign=Gtk.Align.CENTER)
                delete.connect("clicked", lambda *_, i=alarm["id"]: self.edit_alarm(i, delete=True))
                head.pack_start(delete, False, False, 0)
                box.pack_start(head, False, False, 0)

        def add_alarm(self):
            data = load()
            days = [index for index, check in enumerate(self.alarm_days) if check.get_active()]
            data["alarms"].append(new_alarm(f"{int(self.alarm_h.get_value()):02d}:{int(self.alarm_m.get_value()):02d}",
                                            days, self.alarm_name.get_text().strip()))
            save(data)
            self.build_alarm()
            self.pages["alarm"].show_all()

        def edit_alarm(self, ident, on=None, delete=False):
            data = load()
            if delete: data["alarms"] = [a for a in data["alarms"] if a["id"] != ident]
            for alarm in data["alarms"]:
                if alarm["id"] == ident and on is not None:
                    alarm["on"], alarm["snooze"] = on, 0
            save(data)
            if delete:
                self.build_alarm()
                self.pages["alarm"].show_all()

        # Chronomètre
        def build_stopwatch(self):
            page = self.clear("stopwatch")
            self.watch_label = Gtk.Label(xalign=0.5)
            self.watch_label.get_style_context().add_class("big")
            page.pack_start(self.watch_label, False, False, 0)
            row = Gtk.Box(spacing=8, halign=Gtk.Align.CENTER)
            self.watch_toggle = Gtk.Button(label=_t("Démarrer"))
            self.watch_toggle.get_style_context().add_class("accent")
            self.watch_toggle.connect("clicked", lambda *_: self.toggle_watch())
            lap = Gtk.Button(label=_t("Tour"))
            lap.connect("clicked", lambda *_: self.lap())
            reset = Gtk.Button(label=_t("Réinitialiser"))
            reset.connect("clicked", lambda *_: self.reset_watch())
            for widget in (self.watch_toggle, lap, reset): row.pack_start(widget, False, False, 0)
            page.pack_start(row, False, False, 0)
            self.laps_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            page.pack_start(self.laps_box, False, False, 0)

        def watch_elapsed(self):
            running = self.watch["start"] is not None
            return self.watch["elapsed"] + (time.monotonic() - self.watch["start"] if running else 0)

        def toggle_watch(self):
            if self.watch["start"] is None:
                self.watch["start"] = time.monotonic()
                self.watch_toggle.set_label(_t("Pause"))
            else:
                self.watch["elapsed"] = self.watch_elapsed()
                self.watch["start"] = None
                self.watch_toggle.set_label(_t("Reprendre"))

        def lap(self):
            if self.watch["start"] is None: return
            total = self.watch_elapsed()
            self.watch["laps"].append(total)
            previous = self.watch["laps"][-2] if len(self.watch["laps"]) > 1 else 0
            text = f"{_t('Tour {n}').format(n=len(self.watch['laps']))}    {self.watch_text(total - previous)}    {self.watch_text(total)}"
            label = Gtk.Label(label=text, xalign=0)
            self.laps_box.pack_start(label, False, False, 0)
            self.laps_box.reorder_child(label, 0)
            label.show()

        def reset_watch(self):
            self.watch = {"start": None, "elapsed": 0.0, "laps": []}
            self.watch_toggle.set_label(_t("Démarrer"))
            for child in self.laps_box.get_children(): child.destroy()

        @staticmethod
        def watch_text(seconds):
            whole = int(seconds)
            return f"{duration(whole)},{int((seconds - whole) * 100):02d}"

        # Horloges du monde
        def build_world(self):
            page = self.clear("world")
            data = load()
            search = Gtk.SearchEntry(placeholder_text=_t("Rechercher une ville ou un fuseau"))
            completion = Gtk.EntryCompletion()
            store = Gtk.ListStore(str, str)
            for name in zone_names(): store.append([name, name.split("/")[-1].replace("_", " ")])
            completion.set_model(store)
            completion.set_text_column(1)
            completion.set_match_func(lambda _, key, it: key.lower() in store[it][0].lower().replace("_", " "))
            completion.connect("match-selected", lambda _, model, it: self.add_zone(model[it][0]) or False)
            search.set_completion(completion)
            page.pack_start(search, False, False, 0)
            self.world_labels = []
            local = self.card(page)
            label = Gtk.Label(xalign=0)
            label.get_style_context().add_class("mid")
            local.pack_start(Gtk.Label(label=_t("Heure locale"), xalign=0), False, False, 0)
            local.pack_start(label, False, False, 0)
            self.world_labels.append((None, label, None))
            for name in data["zones"]:
                box = self.card(page)
                head = Gtk.Box(spacing=8)
                names = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
                names.pack_start(Gtk.Label(label=name.split("/")[-1].replace("_", " "), xalign=0), False, False, 0)
                offset = Gtk.Label(xalign=0)
                offset.get_style_context().add_class("subtitle")
                names.pack_start(offset, False, False, 0)
                head.pack_start(names, True, True, 0)
                clock = Gtk.Label(xalign=1)
                clock.get_style_context().add_class("mid")
                head.pack_start(clock, False, False, 0)
                delete = Gtk.Button(label=_t("Supprimer"), valign=Gtk.Align.CENTER)
                delete.connect("clicked", lambda *_, n=name: self.remove_zone(n))
                head.pack_start(delete, False, False, 0)
                box.pack_start(head, False, False, 0)
                self.world_labels.append((name, clock, offset))

        def add_zone(self, name):
            data = load()
            if name not in data["zones"]: data["zones"].append(name)
            save(data)
            GLib.idle_add(lambda: (self.build_world(), self.pages["world"].show_all(), False)[-1])

        def remove_zone(self, name):
            data = load()
            data["zones"] = [zone for zone in data["zones"] if zone != name]
            save(data)
            self.build_world()
            self.pages["world"].show_all()

        def tick(self):
            data = load()
            stamp = time.time()
            end = data["focus"].get("end")
            if end:
                self.focus_label.set_text(duration(max(0, end - stamp)))
                if end <= stamp:
                    self.build_focus()
                    self.pages["focus"].show_all()
            else:
                self.focus_label.set_text(duration(self.focus_minutes.get_value() * 60) if hasattr(self, "focus_minutes") else "")
            for timer in data["timers"]:
                label = self.timer_labels.get(timer["id"])
                if label is not None: label.set_text(duration(timer_left(timer, stamp)))
            self.watch_label.set_text(self.watch_text(self.watch_elapsed()))
            utc = dt.datetime.now(dt.timezone.utc)
            local = utc.astimezone()
            for name, clock, offset in self.world_labels:
                moment = local if name is None else zone_time(name, utc)
                if moment is None: continue
                clock.set_text(moment.strftime("%H:%M"))
                if offset is not None:
                    offset.set_text(offset_text(moment.utcoffset().total_seconds(), local.utcoffset().total_seconds()))
            return True

    class App(Gtk.Application):
        def __init__(self):
            super().__init__(application_id="org.winlux.Clock", flags=Gio.ApplicationFlags.FLAGS_NONE)
            self.window = None

        def do_activate(self):
            if self.window is None: self.window = Clock(self)
            self.window.present()

    return App().run([sys.argv[0]])


def main(argv):
    command = argv[1] if len(argv) > 1 else "app"
    if command == "app": return run_app()
    if command == "daemon": return run_daemon()
    data = load()
    if command == "add-alarm" and len(argv) >= 3 and parse_clock(argv[2]):
        days = [int(c) for c in argv[3] if c in "0123456"] if len(argv) > 3 and argv[3] != "-" else []
        data["alarms"].append(new_alarm(argv[2], days, " ".join(argv[4:])))
    elif command == "add-timer" and len(argv) >= 3 and argv[2].isdigit():
        data["timers"].append(new_timer(int(argv[2]), " ".join(argv[3:]), time.time()))
    elif command == "focus" and len(argv) == 3 and argv[2].isdigit():
        focus_start(data, int(argv[2]), time.time())
    elif command == "stop-focus":
        focus_stop(data)
    elif command == "list":
        for alarm in data["alarms"]:
            print(f"alarm {alarm['time']} {''.join(map(str, alarm['days'])) or '-'} {'on' if alarm['on'] else 'off'} {alarm['label']}")
        for timer in data["timers"]:
            print(f"timer {timer['seconds']} {timer_left(timer, time.time())} {timer['label']}")
        if data["focus"].get("end"): print(f"focus {data['focus']['minutes']}")
        return 0
    else:
        print(__doc__.strip().split("\n\n")[-2], file=sys.stderr)
        return 2
    save(data)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
