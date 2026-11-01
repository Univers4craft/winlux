#!/usr/bin/python3
"""Panneaux volants de la barre des tâches, façon Windows 11.

    winlux-popup volume          son : sortie, volume, mélangeur par application
    winlux-popup power           alimentation : batterie, mode, redémarrer/éteindre
    winlux-popup calendar        notifications et calendrier (clic sur l'horloge)
    winlux-popup notifications   même panneau (cloche, Win+N)

Une seule instance par panneau : un second clic sur l'icône le referme. Le
panneau est ancré au coin de la barre (en bas à droite, ou en haut si la barre
est en haut), aux coins arrondis, et se ferme au clic extérieur ou sur Échap.
"""
from pathlib import Path
import calendar
import datetime
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
import gi
gi.require_version("Gtk","3.0")
gi.require_version("Gdk","3.0")
from gi.repository import Gtk,Gdk,Gio,GLib
try:
    import cairo  # python3-gi-cairo : coins arrondis sans compositeur
except ImportError:
    cairo = None


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
    'Application': ('Application', 'Aplicación', 'Anwendung'),
    'Volume Winlux': ('Winlux volume', 'Volumen de Winlux', 'Winlux-Lautstärke'),
    'Alimentation Winlux': ('Winlux power', 'Energía de Winlux', 'Winlux-Energie'),
    'Notifications et calendrier': ('Notifications and calendar', 'Notificaciones y calendario', 'Benachrichtigungen und Kalender'),
    'Volume de sortie': ('Output volume', 'Volumen de salida', 'Ausgabelautstärke'),
    'Volume de sortie, en pourcentage': ('Output volume, as a percentage', 'Volumen de salida, en porcentaje', 'Ausgabelautstärke in Prozent'),
    'Muet / rétablir': ('Mute / unmute', 'Silenciar / restablecer', 'Stumm / Ton an'),
    'Redémarrer': ('Restart', 'Reiniciar', 'Neu starten'),
    'Éteindre': ('Shut down', 'Apagar', 'Herunterfahren'),
    'Paramètres du son': ('Sound settings', 'Configuración de sonido', 'Soundeinstellungen'),
    'Options d’alimentation': ('Power options', 'Opciones de energía', 'Energieoptionen'),
    'Éteindre l’ordinateur ?': ('Shut down the computer?', '¿Apagar el equipo?', 'Computer herunterfahren?'),
    'Redémarrer l’ordinateur ?': ('Restart the computer?', '¿Reiniciar el equipo?', 'Computer neu starten?'),
    'Enregistrez vos documents avant de continuer.': ('Save your documents before continuing.', 'Guarde sus documentos antes de continuar.', 'Speichern Sie Ihre Dokumente, bevor Sie fortfahren.'),
    'Mélangeur de volume': ('Volume mixer', 'Mezclador de volumen', 'Lautstärkemixer'),
    'Aucune sortie audio détectée': ('No audio output found', 'No se detectó ninguna salida de audio', 'Keine Audioausgabe gefunden'),
    'Branchez un casque ou des haut-parleurs, puis rouvrez ce panneau.': ('Plug in headphones or speakers, then open this panel again.', 'Conecte auriculares o altavoces y vuelva a abrir este panel.', 'Schließen Sie Kopfhörer oder Lautsprecher an und öffnen Sie dieses Feld erneut.'),
    'Sortie : ': ('Output: ', 'Salida: ', 'Ausgabe: '),
    'Sortie audio': ('Audio output', 'Salida de audio', 'Audioausgabe'),
    'Volume de {name}': ('{name} volume', 'Volumen de {name}', 'Lautstärke von {name}'),
    'Couper le son de {name}': ('Mute {name}', 'Silenciar {name}', '{name} stummschalten'),
    'Batterie · {level} %': ('Battery · {level}%', 'Batería · {level} %', 'Akku · {level} %'),
    'État de batterie indisponible': ('Battery status unavailable', 'Estado de la batería no disponible', 'Akkustatus nicht verfügbar'),
    'Alimentation secteur': ('Plugged in', 'Conectado a la corriente', 'Netzbetrieb'),
    'Aucune batterie exposée par cet ordinateur': ('This computer reports no battery', 'Este equipo no informa de ninguna batería', 'Dieser Computer meldet keinen Akku'),
    'En charge': ('Charging', 'Cargando', 'Wird geladen'),
    'Sur batterie': ('On battery', 'Con batería', 'Akkubetrieb'),
    'Chargée': ('Fully charged', 'Cargada', 'Vollständig geladen'),
    'Branchée, pas en charge': ('Plugged in, not charging', 'Conectada, sin cargar', 'Angeschlossen, wird nicht geladen'),
    'Mode d’alimentation': ('Power mode', 'Modo de energía', 'Energiemodus'),
    'Économie': ('Saver', 'Ahorro', 'Sparen'),
    'Normal': ('Normal', 'Normal', 'Normal'),
    'Turbo': ('Turbo', 'Turbo', 'Turbo'),
    'Autonomie maximale, processeur ralenti': ('Longest battery life, slower processor', 'Máxima autonomía, procesador más lento', 'Längste Akkulaufzeit, langsamerer Prozessor'),
    'Équilibre entre performances et consommation': ('Balance between performance and power use', 'Equilibrio entre rendimiento y consumo', 'Gleichgewicht zwischen Leistung und Verbrauch'),
    'Performances maximales, consommation plus élevée': ('Best performance, higher power use', 'Máximo rendimiento, mayor consumo', 'Höchste Leistung, höherer Verbrauch'),
    'Mode non appliqué : droits insuffisants ou service arrêté': ('Mode not applied: missing rights or service stopped', 'Modo no aplicado: permisos insuficientes o servicio detenido', 'Modus nicht angewendet: fehlende Rechte oder Dienst gestoppt'),
    'Notifications': ('Notifications', 'Notificaciones', 'Benachrichtigungen'),
    'Tout effacer': ('Clear all', 'Borrar todo', 'Alle löschen'),
    'Ne pas déranger': ('Do not disturb', 'No molestar', 'Nicht stören'),
    'Aucune nouvelle notification': ('No new notifications', 'No hay notificaciones nuevas', 'Keine neuen Benachrichtigungen'),
    'Ignorer': ('Dismiss', 'Descartar', 'Schließen'),
    'Mois précédent': ('Previous month', 'Mes anterior', 'Vorheriger Monat'),
    'Mois suivant': ('Next month', 'Mes siguiente', 'Nächster Monat'),
    'Revenir à aujourd’hui': ('Back to today', 'Volver a hoy', 'Zurück zu heute'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


MONTHS = {
    "fr": "janvier février mars avril mai juin juillet août septembre octobre novembre décembre",
    "en": "January February March April May June July August September October November December",
    "es": "enero febrero marzo abril mayo junio julio agosto septiembre octubre noviembre diciembre",
    "de": "Januar Februar März April Mai Juni Juli August September Oktober November Dezember",
}
DAYS = {  # lundi d'abord
    "fr": "lundi mardi mercredi jeudi vendredi samedi dimanche",
    "en": "Monday Tuesday Wednesday Thursday Friday Saturday Sunday",
    "es": "lunes martes miércoles jueves viernes sábado domingo",
    "de": "Montag Dienstag Mittwoch Donnerstag Freitag Samstag Sonntag",
}
SHORT_DAYS = {"fr": "lu ma me je ve sa di", "en": "Mo Tu We Th Fr Sa Su",
              "es": "lu ma mi ju vi sá do", "de": "Mo Di Mi Do Fr Sa So"}
FIRST_WEEKDAY = 6 if _LANG == "en" else 0  # dimanche aux États-Unis, lundi ailleurs


def today():
    """Date du jour ; WINLUX_TODAY=AAAA-MM-JJ la fige (captures reproductibles)."""
    fixed = os.environ.get("WINLUX_TODAY", "")
    try:
        return time.strptime(fixed, "%Y-%m-%d") if fixed else time.localtime()
    except ValueError:
        return time.localtime()


def month_title(year, month):
    name = MONTHS[_LANG].split()[month - 1]
    return f"{name} {year}" if _LANG != "es" else f"{name} de {year}"


def long_date(day):
    weekday = DAYS[_LANG].split()[day.tm_wday]
    month = MONTHS[_LANG].split()[day.tm_mon - 1]
    return {"en": f"{weekday}, {month} {day.tm_mday}", "es": f"{weekday}, {day.tm_mday} de {month}",
            "de": f"{weekday}, {day.tm_mday}. {month}"}.get(_LANG, f"{weekday} {day.tm_mday} {month}")


# ── Son : sortie par défaut et flux des applications ────────────────────────

def _run(args, timeout=2):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
        return result.stdout if result.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


def _percent(volume):
    levels = []
    for channel in (volume or {}).values():
        try: levels.append(int(str(channel.get("value_percent", "0")).strip().rstrip("%")))
        except (ValueError, AttributeError): pass
    return max(levels or [0])


def audio():
    """(moteur, volume, muet, nom de la sortie) ; moteur None sans sortie réelle.

    PipeWire/PulseAudio d'abord (la sortie « auto_null » est le puits factice
    créé quand aucune carte n'existe : ce n'est pas une sortie), puis ALSA."""
    if shutil.which("pactl"):
        default = (_run(["pactl", "get-default-sink"]) or "").strip()
        sinks = _run(["pactl", "-f", "json", "list", "sinks"])
        if default and default != "auto_null" and sinks:
            try:
                for sink in json.loads(sinks):
                    if sink.get("name") == default:
                        return ("pactl", min(100, _percent(sink.get("volume"))), bool(sink.get("mute")),
                                sink.get("description") or default)
            except (ValueError, TypeError, AttributeError):
                pass
    if shutil.which("wpctl"):
        output = _run(["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"])
        found = re.search(r"Volume:\s*([0-9.]+)", output or "")
        if found:
            inspect = _run(["wpctl", "inspect", "@DEFAULT_AUDIO_SINK@"]) or ""
            name = re.search(r'node\.description = "([^"]*)"', inspect)
            return ("wpctl", min(100, round(float(found[1]) * 100)), "MUTED" in output,
                    name[1] if name else _t("Sortie audio"))
    if shutil.which("amixer"):
        output = _run(["amixer", "get", "Master"])
        found = re.findall(r"\[(\d+)%\]", output or "")
        if found:
            card = ""
            try:
                first = Path("/proc/asound/cards").read_text(errors="replace").splitlines()[0]
                card = first.split(" - ", 1)[1].strip() if " - " in first else ""
            except (OSError, IndexError):
                pass
            return ("amixer", int(found[-1]), "[off]" in output, card or _t("Sortie audio"))
    return (None, 0, False, "")


def app_streams():
    """Applications qui émettent du son en ce moment (programmes .exe compris :
    Winlux nomme chaque flux d'après son exécutable). Les flux en pause
    (« corked ») n'ont pas de barre, comme dans Windows 11."""
    if not shutil.which("pactl"):
        return []
    output = _run(["pactl", "-f", "json", "list", "sink-inputs"])
    streams = []
    try:
        for entry in json.loads(output or "[]"):
            if entry.get("corked"):
                continue
            props = entry.get("properties", {})
            if props.get("media.role") == "event":
                continue  # sons système brefs
            name = (props.get("application.name") or props.get("application.process.binary")
                    or props.get("media.name") or _t("Application"))
            icon = props.get("application.icon_name") or ""
            streams.append((int(entry["index"]), name, icon, min(100, _percent(entry.get("volume"))), bool(entry.get("mute"))))
    except (ValueError, KeyError, TypeError, AttributeError):
        return []
    return streams


def volume_icon(level, muted):
    if muted or level == 0: return "audio-volume-muted-symbolic"
    return "audio-volume-low-symbolic" if level < 34 else "audio-volume-medium-symbolic" if level < 67 else "audio-volume-high-symbolic"


def power_modes():
    """Module winlux-power-modes installé à côté (ou dans le dépôt)."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (here.with_name("winlux-power-modes.py"), here.with_name("winlux-power-modes")):
        if candidate.is_file():
            loader = SourceFileLoader("winlux_power_modes", str(candidate))
            spec = importlib.util.spec_from_loader("winlux_power_modes", loader)
            module = importlib.util.module_from_spec(spec)
            loader.exec_module(module)
            return module
    return None


STATE = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "winlux/notifications.json"
DND = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "winlux/do-not-disturb"


# ── Apparence ───────────────────────────────────────────────────────────────

# Même palette que winlux-start-menu.py (contraste vérifié par test-shell-contrast.py).
PALETTES = {
    "dark": {"bg": "#242238", "fg": "#f7f5ff", "border": "#6b5f92", "dim": "#c4bee0", "button": "#322e4a",
             "hover": "#433b60", "disabled": "#9a93b8", "accent": "#b19af9", "on_accent": "#1b1530",
             "trough": "#4a4466", "card": "#2e2b45"},
    "light": {"bg": "#f6f4fc", "fg": "#1f1b2e", "border": "#c9c0e6", "dim": "#5e5778", "button": "#ffffff",
              "hover": "#e4ddf7", "disabled": "#6c6585", "accent": "#6a3df0", "on_accent": "#ffffff",
              "trough": "#d9d2ee", "card": "#ffffff"},
}
POPUP_CSS = Template("""
  #winlux-popup { background: $bg; color: $fg; }
  #winlux-popup .panel { background: $bg; color: $fg; border: 1px solid $border; border-radius: 8px; }
  #winlux-popup box, #winlux-popup grid, #winlux-popup scrolledwindow, #winlux-popup viewport,
  #winlux-popup list, #winlux-popup row { background: transparent; border: none; }
  #winlux-popup label, #winlux-popup image, #winlux-popup scale value { color: $fg; text-shadow: none; }
  #winlux-popup .title { font-weight: bold; }
  #winlux-popup .dim, #winlux-popup .dim label { color: $dim; }
  #winlux-popup .footer { background: $card; border-top: 1px solid $border; border-radius: 0 0 8px 8px; }
  #winlux-popup button { padding: 6px 10px; border-radius: 5px; background: $button; color: $fg;
                         border: 1px solid $border; box-shadow: none; text-shadow: none; }
  #winlux-popup button:hover { background: $hover; }
  #winlux-popup button.flat { background: transparent; border-color: transparent; }
  #winlux-popup button.flat:hover { background: $hover; }
  #winlux-popup button.header { padding: 2px 0; }
  #winlux-popup button.icon { padding: 6px; min-width: 20px; min-height: 20px; }
  #winlux-popup button:disabled, #winlux-popup button:disabled label, #winlux-popup scale:disabled value { color: $disabled; }
  #winlux-popup button.mode { padding: 10px 6px; }
  #winlux-popup button.mode:checked { background: $accent; border-color: $accent; }
  #winlux-popup button.mode:checked label, #winlux-popup button.mode:checked image { color: $on_accent; }
  #winlux-popup scale trough { min-height: 4px; background: $trough; border: none; border-radius: 2px; }
  #winlux-popup scale highlight { background: $accent; border-radius: 2px; }
  #winlux-popup scale slider { background: $accent; border: 3px solid $bg; min-width: 18px; min-height: 18px;
                               border-radius: 50%; box-shadow: 0 0 0 1px $border; }
  #winlux-popup .card { background: $card; border: 1px solid $border; border-radius: 6px; }
  #winlux-popup switch { background: $trough; border: 1px solid $border; }
  #winlux-popup switch:checked { background: $accent; }
  #winlux-popup switch slider { background: $fg; border: none; }
  #winlux-popup switch:checked slider { background: $on_accent; }
  #winlux-popup button.day { padding: 0; min-width: 40px; min-height: 40px; border-radius: 50%;
                             background: transparent; border: 1px solid transparent; }
  #winlux-popup button.day:hover { background: $hover; }
  #winlux-popup button.day.other label { color: $dim; }
  #winlux-popup button.day.selected { border-color: $accent; }
  #winlux-popup button.day.today { background: $accent; border-color: $accent; }
  #winlux-popup button.day.today label { color: $on_accent; font-weight: bold; }
  #winlux-popup .weekday { color: $dim; font-size: smaller; }
""")


def dark_theme():
    forced=os.environ.get("GTK_THEME")
    if forced:return "dark" in forced.casefold()  # GTK ignore alors la préférence
    settings=Gtk.Settings.get_default()
    name=(settings.get_property("gtk-theme-name") or "").casefold()
    return bool(settings.get_property("gtk-application-prefer-dark-theme")) or "dark" in name


def reveal(widget, visible):
    """Affiche ou cache entièrement une section (jamais grisée ni vide)."""
    if visible:
        widget.set_no_show_all(False); widget.show_all(); widget.set_no_show_all(True)
    else:
        widget.hide()


def rounded(region, x, y, width, height, radius):
    """Ajoute un rectangle aux coins arrondis à une région cairo (ligne à ligne)."""
    if width <= 0 or height <= 0: return
    radius = min(radius, width // 2, height // 2)
    region.union(cairo.RectangleInt(x, y + radius, width, height - 2 * radius))
    for row in range(radius):
        inset = radius - int(round(math.sqrt(radius * radius - (radius - row - 0.5) ** 2)))
        region.union(cairo.RectangleInt(x + inset, y + row, width - 2 * inset, 1))
        region.union(cairo.RectangleInt(x + inset, y + height - 1 - row, width - 2 * inset, 1))


class Popup(Gtk.Application):
    KINDS = {"volume": "volume", "power": "power", "calendar": "center", "notifications": "center", "center": "center"}

    def __init__(self,kind):
        self.kind=self.KINDS[kind]
        super().__init__(application_id="org.winlux.Popup."+self.kind,flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.window=None;self.timer=0;self.child=None;self.backend=None
        self.updating=False;self.anchor=None;self.modes_busy=False

    # ── Ouverture, ancrage et fermeture ─────────────────────────────────────

    def hide(self,*_):
        if self.window.has_grab():self.window.grab_remove()
        self.window.get_display().get_default_seat().ungrab()
        self.window.hide()
        return True

    def activate_seat(self):
        if self.window.get_visible():
            self.pointer_status=self.window.get_display().get_default_seat().grab(
                self.window.get_window(),Gdk.SeatCapabilities.POINTER,False,None,None,None,None)
            if self.pointer_status==Gdk.GrabStatus.SUCCESS:self.window.grab_add()
        return False

    def outside(self,_,event):
        x,y=self.window.get_position();width,height=self.window.get_size()
        if not (x<=event.x_root<x+width and y<=event.y_root<y+height):return self.hide()
        return False

    def do_activate(self):
        if self.window and self.window.get_visible():self.hide();return
        if not self.window:self.build()
        self.on_open()
        self.window.show_all()
        self.refresh()
        self.window.present()
        display=self.window.get_display()
        monitor=display.get_monitor_at_point(*display.get_default_seat().get_pointer().get_position()[1:])
        self.anchor=(monitor.get_workarea(),monitor.get_geometry())
        self.place()
        GLib.idle_add(self.activate_seat)

    def place(self,*_):
        """Coin de l'écran côté barre : en bas à droite, en haut si la barre y est."""
        if not self.anchor or not self.window.get_visible():return
        area,screen=self.anchor
        width,height=self.window.get_size()
        top=area.y>screen.y  # la zone utile commence sous une barre placée en haut
        x=area.x+area.width-width-12
        y=area.y+12 if top else area.y+area.height-height-12
        if self.window.get_position()!=(x,y):self.window.move(x,y)

    def shape(self,widget,allocation):
        """Coins arrondis sans compositeur : la fenêtre prend la forme de ses panneaux."""
        if cairo is None:return
        region=cairo.Region()
        for panel in self.panels:
            if not panel.get_visible():continue
            child=panel.get_allocation()
            rounded(region,child.x,child.y,child.width,child.height,8)
        if not region.is_empty():
            self.window.shape_combine_region(region)
        GLib.idle_add(self.place)

    def build(self):
        titles={"volume":_t("Volume Winlux"),"power":_t("Alimentation Winlux"),"center":_t("Notifications et calendrier")}
        self.window=Gtk.ApplicationWindow(application=self,title=titles[self.kind])
        self.window.set_name("winlux-popup")
        # Même palette que le menu Démarrer, au-dessus du gtk.css utilisateur.
        self.css=Gtk.CssProvider()
        Gtk.StyleContext.add_provider_for_screen(self.window.get_screen(),self.css,Gtk.STYLE_PROVIDER_PRIORITY_USER+1)
        settings=Gtk.Settings.get_default()
        for name in ("gtk-theme-name","gtk-application-prefer-dark-theme"):
            settings.connect("notify::"+name,lambda *_:self.apply_theme())
        self.apply_theme()
        self.window.set_decorated(False);self.window.set_resizable(False)
        self.window.set_skip_taskbar_hint(True);self.window.set_skip_pager_hint(True)
        self.window.set_type_hint(Gdk.WindowTypeHint.UTILITY);self.window.set_keep_above(True)
        self.window.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        self.window.connect("button-press-event",self.outside)
        self.window.connect("key-press-event",lambda _,event:self.hide() if event.keyval==Gdk.KEY_Escape else False)
        self.window.connect("focus-out-event",lambda *_:self.hide())
        self.outer=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=12)
        self.window.add(self.outer)
        self.panels=[]
        getattr(self,"build_"+self.kind)()
        self.window.connect("size-allocate",self.shape)
        GLib.timeout_add_seconds(1,self.refresh)

    def panel(self,width=360):
        frame=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=0)
        frame.get_style_context().add_class("panel")
        frame.set_size_request(width,-1)
        body=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=12,margin=16)
        frame.pack_start(body,True,True,0)
        self.outer.pack_start(frame,False,False,0)
        self.panels.append(frame)
        return frame,body

    def footer(self,frame,label,callback):
        bar=Gtk.Box(spacing=8);bar.get_style_context().add_class("footer")
        inner=Gtk.Box(spacing=8,margin=10,margin_start=16,margin_end=16);bar.pack_start(inner,True,True,0)
        button=Gtk.Button(label=label);button.get_style_context().add_class("flat")
        button.connect("clicked",lambda *_:callback())
        inner.pack_end(button,False,False,0)
        frame.pack_end(bar,False,False,0)
        return inner

    def label(self,text="",*classes,**props):
        props.setdefault("xalign",0)
        widget=Gtk.Label(label=text,**props)
        for name in classes:widget.get_style_context().add_class(name)
        return widget

    def icon_button(self,icon,tooltip,callback,*classes):
        button=Gtk.Button.new_from_icon_name(icon,Gtk.IconSize.BUTTON)
        for name in ("icon","flat")+classes:button.get_style_context().add_class(name)
        button.set_tooltip_text(tooltip);button.get_accessible().set_name(tooltip)
        button.connect("clicked",lambda *_:callback())
        return button

    def apply_theme(self):
        self.css.load_from_data(POPUP_CSS.substitute(PALETTES["dark" if dark_theme() else "light"]).encode())

    def on_open(self):
        if self.kind=="power":self.refresh_modes()
        if self.kind=="center":
            self.shown_month=None;self.selected=None
            now=today();self.month=(now.tm_year,now.tm_mon)

    def command(self,args):
        if self.child and self.child.poll() is None:return
        self.child=subprocess.Popen(args,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

    def settings(self,page):
        self.hide();subprocess.Popen(["winlux-shell-app","settings",page])

    def refresh(self):
        if not self.window or not self.window.get_visible():return True
        getattr(self,"refresh_"+self.kind)()
        return True

    # ── Son ─────────────────────────────────────────────────────────────────

    def build_volume(self):
        frame,body=self.panel()
        # Sortie absente : message seul, sans curseur grisé (la section est cachée).
        self.missing=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=4)
        self.missing.pack_start(self.label(_t("Aucune sortie audio détectée"),"title"),False,False,0)
        self.missing.pack_start(self.label(_t("Branchez un casque ou des haut-parleurs, puis rouvrez ce panneau."),"dim",wrap=True,max_width_chars=40),False,False,0)
        self.missing.set_no_show_all(True)
        body.pack_start(self.missing,False,False,0)
        self.output=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=6)
        row=Gtk.Box(spacing=6)
        self.mute=self.icon_button("audio-volume-high-symbolic",_t("Muet / rétablir"),self.toggle_mute)
        self.slider=Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL,0,100,1)
        self.slider.set_draw_value(False);self.slider.set_hexpand(True)
        self.slider.set_tooltip_text(_t("Volume de sortie"))
        self.slider.get_accessible().set_name(_t("Volume de sortie, en pourcentage"))
        self.slider.connect("value-changed",self.change_volume)
        self.value=self.label("",width_chars=4,xalign=1)
        row.pack_start(self.mute,False,False,0);row.pack_start(self.slider,True,True,0);row.pack_start(self.value,False,False,0)
        self.output.pack_start(row,False,False,0)
        self.device=self.label("","dim",ellipsize=3,max_width_chars=40)
        self.output.pack_start(self.device,False,False,0)
        # Mélangeur : une barre par application qui joue du son en ce moment.
        self.apps=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=8)
        self.output.pack_start(self.apps,False,False,6)
        self.output.set_no_show_all(True)
        body.pack_start(self.output,False,False,0)
        self.apps_key=None;self.app_rows={}
        self.footer(frame,_t("Paramètres du son"),lambda:self.settings("3"))

    def toggle_mute(self):
        if self.backend=="pactl":self.command(["pactl","set-sink-mute","@DEFAULT_SINK@","toggle"])
        elif self.backend=="wpctl":self.command(["wpctl","set-mute","@DEFAULT_AUDIO_SINK@","toggle"])
        elif self.backend=="amixer":self.command(["amixer","set","Master","toggle"])
        GLib.timeout_add(250,lambda:(self.refresh(),False)[1])

    def change_volume(self,slider):
        self.value.set_text(f"{round(slider.get_value())}")
        if self.updating:return
        if self.timer:GLib.source_remove(self.timer)
        self.timer=GLib.timeout_add(120,self.set_volume)

    def set_volume(self):
        self.timer=0
        if self.child and self.child.poll() is None:
            self.timer=GLib.timeout_add(60,self.set_volume)
            return False
        value=round(self.slider.get_value())
        if self.backend=="pactl":self.command(["pactl","set-sink-volume","@DEFAULT_SINK@",f"{value}%"])
        elif self.backend=="wpctl":self.command(["wpctl","set-volume","-l","1","@DEFAULT_AUDIO_SINK@",f"{value}%"])
        elif self.backend=="amixer":self.command(["amixer","set","Master",f"{value}%"])
        return False

    def app_row(self,index,name,icon):
        box=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=2)
        head=Gtk.Box(spacing=8)
        image=Gtk.Image.new_from_icon_name(icon if icon and Gtk.IconTheme.get_default().has_icon(icon)
                                           else "application-x-executable-symbolic",Gtk.IconSize.MENU)
        head.pack_start(image,False,False,0)
        head.pack_start(self.label(name,ellipsize=3,max_width_chars=34),True,True,0)
        box.pack_start(head,False,False,0)
        row=Gtk.Box(spacing=6)
        mute=self.icon_button("audio-volume-high-symbolic",_t("Couper le son de {name}").format(name=name),
                              lambda:subprocess.Popen(["pactl","set-sink-input-mute",str(index),"toggle"],
                                                      stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
        scale=Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL,0,100,1)
        scale.set_draw_value(False);scale.set_hexpand(True)
        scale.get_accessible().set_name(_t("Volume de {name}").format(name=name))
        value=self.label("",width_chars=4,xalign=1)
        def changed(scale,index=index,value=value):
            value.set_text(f"{round(scale.get_value())}")
            if not self.updating:
                subprocess.Popen(["pactl","set-sink-input-volume",str(index),f"{round(scale.get_value())}%"],
                                 stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        scale.connect("value-changed",changed)
        row.pack_start(mute,False,False,0);row.pack_start(scale,True,True,0);row.pack_start(value,False,False,0)
        box.pack_start(row,False,False,0)
        return box,mute,scale,value

    def refresh_apps(self):
        streams=app_streams()
        key=tuple((index,name) for index,name,_,_,_ in streams)
        if key!=self.apps_key:
            self.apps_key=key;self.app_rows={}
            for child in self.apps.get_children():self.apps.remove(child)
            if streams:self.apps.pack_start(self.label(_t("Mélangeur de volume"),"title"),False,False,0)
            for index,name,icon,_,_ in streams:
                box,mute,scale,value=self.app_row(index,name,icon)
                self.apps.pack_start(box,False,False,0);self.app_rows[index]=(mute,scale,value)
            self.apps.show_all()
        for index,_,_,level,muted in streams:
            mute,scale,value=self.app_rows.get(index,(None,None,None))
            if not scale:continue
            mute.get_image().set_from_icon_name(volume_icon(level,muted),Gtk.IconSize.BUTTON)
            if not scale.get_state_flags() & Gtk.StateFlags.ACTIVE:
                self.updating=True;scale.set_value(level);value.set_text(f"{level}");self.updating=False

    def refresh_volume(self):
        self.backend,value,muted,device=audio()
        reveal(self.missing,not self.backend)
        if not self.backend:
            self.output.hide()
            return
        if not self.output.get_visible():reveal(self.output,True)
        self.mute.get_image().set_from_icon_name(volume_icon(value,muted),Gtk.IconSize.BUTTON)
        self.device.set_text(_t("Sortie : ")+device)
        if not self.timer and not (self.child and self.child.poll() is None) and not self.slider.get_state_flags() & Gtk.StateFlags.ACTIVE:
            self.updating=True;self.slider.set_value(value);self.value.set_text(f"{value}");self.updating=False
        self.refresh_apps()

    # ── Alimentation ────────────────────────────────────────────────────────

    MODE_LABELS = (("saver", "Économie", "power-profile-power-saver-symbolic", "Autonomie maximale, processeur ralenti"),
                   ("balanced", "Normal", "power-profile-balanced-symbolic", "Équilibre entre performances et consommation"),
                   ("performance", "Turbo", "power-profile-performance-symbolic", "Performances maximales, consommation plus élevée"))

    def build_power(self):
        frame,body=self.panel()
        self.battery=self.label("","title");self.battery_detail=self.label("","dim")
        body.pack_start(self.battery,False,False,0);body.pack_start(self.battery_detail,False,False,0)
        # Mode d'alimentation : seulement les modes que la machine sait appliquer.
        self.modes=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=8)
        self.modes.pack_start(self.label(_t("Mode d’alimentation"),"title"),False,False,0)
        self.mode_row=Gtk.Box(spacing=6,homogeneous=True)
        self.modes.pack_start(self.mode_row,False,False,0)
        self.mode_hint=self.label("","dim",wrap=True,max_width_chars=40)
        self.modes.pack_start(self.mode_hint,False,False,0)
        self.modes.set_no_show_all(True)
        body.pack_start(self.modes,False,False,4)
        self.mode_buttons={};self.mode_state=(None,[],None);self.modes_module=power_modes()
        actions=Gtk.Box(spacing=8,homogeneous=True)
        for title,icon,action in [(_t("Redémarrer"),"system-reboot-symbolic","reboot"),(_t("Éteindre"),"system-shutdown-symbolic","poweroff")]:
            button=Gtk.Button(label=title,image=Gtk.Image.new_from_icon_name(icon,Gtk.IconSize.BUTTON),always_show_image=True)
            button.connect("clicked",lambda _,value=action:self.power_action(value))
            actions.pack_start(button,True,True,0)
        body.pack_start(actions,False,False,0)
        self.footer(frame,_t("Options d’alimentation"),lambda:self.settings("23"))

    def refresh_modes(self):
        """Détection en arrière-plan (powerprofilesctl/tuned-adm peuvent tarder)."""
        if self.modes_busy or not self.modes_module:return
        self.modes_busy=True
        def worker():
            try:state=self.modes_module.detect()
            except Exception:state=(None,[],None)
            GLib.idle_add(self.show_modes,state)
        threading.Thread(target=worker,daemon=True).start()

    def show_modes(self,state):
        self.modes_busy=False
        self.mode_state=state
        _,available,active=state
        if [m for m in self.mode_buttons]!=available:
            for child in self.mode_row.get_children():self.mode_row.remove(child)
            self.mode_buttons={}
            for mode,title,icon,_ in self.MODE_LABELS:
                if mode not in available:continue
                button=Gtk.ToggleButton()
                content=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=4)
                theme=Gtk.IconTheme.get_default()
                content.pack_start(Gtk.Image.new_from_icon_name(icon if theme.has_icon(icon) else "battery-good-symbolic",Gtk.IconSize.LARGE_TOOLBAR),False,False,0)
                content.pack_start(Gtk.Label(label=_t(title)),False,False,0)
                button.add(content);button.get_style_context().add_class("mode")
                button.get_accessible().set_name(_t(title))
                button.connect("toggled",lambda b,value=mode:self.choose_mode(b,value))
                self.mode_row.pack_start(button,True,True,0);self.mode_buttons[mode]=button
        self.updating=True
        for mode,button in self.mode_buttons.items():button.set_active(mode==active)
        self.updating=False
        hint=next((_t(text) for mode,_,_,text in self.MODE_LABELS if mode==active),"")
        if self.mode_hint.get_text()!=_t("Mode non appliqué : droits insuffisants ou service arrêté") or active:
            self.mode_hint.set_text(hint)
        reveal(self.modes,bool(available))
        return False

    def choose_mode(self,button,mode):
        if self.updating:return
        if not button.get_active():
            self.updating=True;button.set_active(True);self.updating=False  # un mode reste toujours actif
            return
        self.updating=True
        for other,widget in self.mode_buttons.items():widget.set_active(other==mode)
        self.updating=False
        source=self.mode_state[0]
        def worker():
            try:done=self.modes_module.apply(mode,source)
            except Exception:done=False
            GLib.idle_add(lambda:(self.mode_hint.set_text("" if done else _t("Mode non appliqué : droits insuffisants ou service arrêté")),
                                  self.refresh_modes(),False)[-1])
        threading.Thread(target=worker,daemon=True).start()

    def power_action(self,action):
        self.hide()
        dialog=Gtk.MessageDialog(transient_for=self.window,modal=True,
            buttons=Gtk.ButtonsType.OK_CANCEL,text=_t("Éteindre l’ordinateur ?") if action=="poweroff" else _t("Redémarrer l’ordinateur ?"),
            secondary_text=_t("Enregistrez vos documents avant de continuer."))
        answer=dialog.run();dialog.destroy()
        if answer==Gtk.ResponseType.OK:self.command(["winlux-control",action])

    def refresh_power(self):
        batteries=sorted(Path("/sys/class/power_supply").glob("BAT*/capacity"))
        if batteries:
            try:
                level=int(batteries[0].read_text())
                status=(batteries[0].parent/"status").read_text().strip()
                self.battery.set_text(_t("Batterie · {level} %").format(level=level))
                self.battery_detail.set_text(_t({"Charging":"En charge","Discharging":"Sur batterie","Full":"Chargée",
                                                 "Not charging":"Branchée, pas en charge"}.get(status,"Sur batterie")))
            except (OSError,ValueError):
                self.battery.set_text(_t("État de batterie indisponible"));self.battery_detail.set_text("")
        else:
            self.battery.set_text(_t("Alimentation secteur"))
            self.battery_detail.set_text(_t("Aucune batterie exposée par cet ordinateur"))
        self.power_ticks=getattr(self,"power_ticks",0)+1
        if self.power_ticks%5==0:self.refresh_modes()  # mode changé ailleurs (Paramètres)

    # ── Notifications et calendrier ─────────────────────────────────────────

    def build_center(self):
        frame,body=self.panel()
        head=Gtk.Box(spacing=8)
        head.pack_start(self.label(_t("Notifications"),"title"),True,True,0)
        self.clear=Gtk.Button(label=_t("Tout effacer"));self.clear.get_style_context().add_class("flat")
        self.clear.connect("clicked",lambda *_:self.notification_call("Clear"))
        head.pack_end(self.clear,False,False,0)
        body.pack_start(head,False,False,0)
        quiet=Gtk.Box(spacing=8)
        quiet.pack_start(self.label(_t("Ne pas déranger")),True,True,0)
        self.dnd=Gtk.Switch(valign=Gtk.Align.CENTER);self.dnd.get_accessible().set_name(_t("Ne pas déranger"))
        self.dnd.connect("notify::active",self.toggle_dnd)
        quiet.pack_end(self.dnd,False,False,0)
        body.pack_start(quiet,False,False,0)
        self.scroll=Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
        self.scroll.set_propagate_natural_height(True);self.scroll.set_max_content_height(340)
        self.cards=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=8)
        self.scroll.add(self.cards)
        body.pack_start(self.scroll,False,False,0)
        self.snapshot=None
        # Calendrier : panneau séparé sous les notifications, comme Windows 11.
        frame,body=self.panel()
        self.today_button=Gtk.Button(halign=Gtk.Align.START)
        for name in ("flat","header"):self.today_button.get_style_context().add_class(name)
        self.today_label=self.label("","title");self.today_button.add(self.today_label)
        self.today_button.set_tooltip_text(_t("Revenir à aujourd’hui"))
        self.today_button.connect("clicked",lambda *_:self.go_today())
        body.pack_start(self.today_button,False,False,0)
        nav=Gtk.Box(spacing=4)
        self.month_label=self.label("","title")
        nav.pack_start(self.month_label,True,True,0)
        nav.pack_end(self.icon_button("pan-down-symbolic",_t("Mois suivant"),lambda:self.shift_month(1)),False,False,0)
        nav.pack_end(self.icon_button("pan-up-symbolic",_t("Mois précédent"),lambda:self.shift_month(-1)),False,False,0)
        body.pack_start(nav,False,False,0)
        self.grid=Gtk.Grid(column_spacing=4,row_spacing=2,halign=Gtk.Align.CENTER)
        events=Gtk.EventBox();events.add(self.grid)
        events.add_events(Gdk.EventMask.SCROLL_MASK|Gdk.EventMask.SMOOTH_SCROLL_MASK)
        events.connect("scroll-event",self.scroll_month)
        body.pack_start(events,False,False,0)
        self.days=[]
        names=SHORT_DAYS[_LANG].split()
        for column in range(7):
            name=names[(column+FIRST_WEEKDAY)%7]
            self.grid.attach(self.label(name,"weekday",xalign=0.5),column,0,1,1)
        for row in range(6):
            for column in range(7):
                button=Gtk.Button(label="");button.get_style_context().add_class("day")
                button.connect("clicked",self.pick_day)
                self.grid.attach(button,column,row+1,1,1);self.days.append(button)

    def notification_call(self,method,parameters=None):
        try:
            Gio.bus_get_sync(Gio.BusType.SESSION,None).call_sync("org.freedesktop.Notifications",
                "/org/freedesktop/Notifications","org.winlux.Notifications",method,
                parameters,None,Gio.DBusCallFlags.NONE,3000,None)
        except GLib.Error:
            pass
        self.snapshot=None;self.refresh()

    def toggle_dnd(self,switch,_):
        if self.updating:return
        try:
            DND.parent.mkdir(parents=True,exist_ok=True)
            if switch.get_active():DND.touch(mode=0o600)
            else:DND.unlink(missing_ok=True)
        except OSError:
            pass

    def notification_card(self,item):
        card=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=4);card.get_style_context().add_class("card")
        inner=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=4,margin=10);card.pack_start(inner,True,True,0)
        head=Gtk.Box(spacing=6)
        stamp=time.localtime(item.get("time",0))
        when=time.strftime("%H:%M" if stamp[:3]==today()[:3] else "%d/%m %H:%M",stamp)
        head.pack_start(self.label(f"{item.get('app') or _t('Application')} · {when}","dim",ellipsize=3,max_width_chars=30),True,True,0)
        head.pack_end(self.icon_button("window-close-symbolic",_t("Ignorer"),
                                       lambda identity=item.get("id",0):self.notification_call("Dismiss",GLib.Variant("(u)",(identity,)))),False,False,0)
        inner.pack_start(head,False,False,0)
        if item.get("title"):inner.pack_start(self.label(item["title"],"title",wrap=True,max_width_chars=36),False,False,0)
        if item.get("body"):inner.pack_start(self.label(item["body"],wrap=True,max_width_chars=36,lines=4,ellipsize=3),False,False,0)
        actions=item.get("actions",[])
        if len(actions)>=2:
            row=Gtk.Box(spacing=6)
            for index in range(0,len(actions)-1,2):
                button=Gtk.Button(label=actions[index+1])
                button.connect("clicked",lambda _,identity=item.get("id",0),action=actions[index]:
                               self.notification_call("Activate",GLib.Variant("(us)",(identity,action))))
                row.pack_start(button,False,False,0)
            inner.pack_start(row,False,False,0)
        return card

    def refresh_notifications(self):
        try:text=STATE.read_text(encoding="utf-8")
        except OSError:text=""
        self.updating=True;self.dnd.set_active(DND.exists());self.updating=False
        if text==self.snapshot:return
        self.snapshot=text
        try:history=json.loads(text or "[]")
        except ValueError:history=[]
        if not isinstance(history,list):history=[]
        for child in self.cards.get_children():self.cards.remove(child)
        for item in reversed(history[-50:]):
            if isinstance(item,dict):self.cards.pack_start(self.notification_card(item),False,False,0)
        if not self.cards.get_children():
            self.cards.pack_start(self.label(_t("Aucune nouvelle notification"),"dim",xalign=0.5,margin=18),False,False,0)
        self.clear.set_visible(bool(history))
        self.clear.set_no_show_all(not history)
        self.cards.show_all()

    def go_today(self):
        now=today();self.month=(now.tm_year,now.tm_mon);self.selected=None;self.render_month()

    def shift_month(self,step):
        year,month=self.month
        month+=step
        if month<1:year,month=year-1,12
        if month>12:year,month=year+1,1
        self.month=(year,month);self.render_month()

    def scroll_month(self,_,event):
        direction=event.direction
        if direction==Gdk.ScrollDirection.SMOOTH:
            delta=event.get_scroll_deltas()[2]
            if abs(delta)<0.5:return True
            self.shift_month(1 if delta>0 else -1)
        elif direction in (Gdk.ScrollDirection.UP,Gdk.ScrollDirection.DOWN):
            self.shift_month(-1 if direction==Gdk.ScrollDirection.UP else 1)
        return True

    def pick_day(self,button):
        day=getattr(button,"winlux_date",None)
        if not day:return
        self.selected=day
        if day[:2]!=self.month:self.month=day[:2]
        self.render_month()

    def render_month(self):
        now=today()
        self.today_label.set_text(long_date(now))
        year,month=self.month
        self.month_label.set_text(month_title(year,month))
        weeks=calendar.Calendar(FIRST_WEEKDAY).monthdatescalendar(year,month)
        while len(weeks)<6:  # grille fixe de six semaines : le panneau ne saute pas
            last=weeks[-1][-1]
            weeks.append([last+datetime.timedelta(days=offset) for offset in range(1,8)])
        for index,button in enumerate(self.days):
            day=weeks[index//7][index%7]
            key=(day.year,day.month,day.day)
            button.winlux_date=key
            button.set_label(str(day.day))
            style=button.get_style_context()
            for name,state in (("other",day.month!=month),("today",key==tuple(now[:3])),("selected",key==self.selected)):
                (style.add_class if state else style.remove_class)(name)
            button.get_accessible().set_name(f"{day.day} {MONTHS[_LANG].split()[day.month-1]} {day.year}")
        self.shown_month=(self.month,tuple(now[:3]),self.selected)

    def refresh_center(self):
        self.refresh_notifications()
        day=tuple(today()[:3])
        if self.shown_month is None or self.shown_month!=(self.month,day,self.selected):self.render_month()


if __name__=="__main__":
    kind=sys.argv[1] if len(sys.argv)>1 else "volume"
    if kind not in Popup.KINDS:raise SystemExit(2)
    raise SystemExit(Popup(kind).run([]))
