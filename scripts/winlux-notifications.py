#!/usr/bin/python3
"""Session notification service with bounded history and explicit actions."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, Gtk, Gio, GLib

STATE = Path(os.environ.get("XDG_STATE_HOME",Path.home()/".local/state")) / "winlux/notifications.json"
PREF = Path(os.environ.get("XDG_CONFIG_HOME",Path.home()/".config")) / "winlux/do-not-disturb"
SOUNDS_OFF = PREF.with_name("system-sounds-off")
SOUNDS = Path(os.environ.get("WINLUX_SOUNDS_DIR", "/usr/share/winlux/sounds"))

def play_sound(name):
    """Son système Winlux (assets/sounds), sans bloquer ni échouer."""
    path = SOUNDS / f"{name}.wav"
    if SOUNDS_OFF.exists() or not path.is_file():
        return False
    for player in ("pw-play", "paplay", "aplay"):
        exe = shutil.which(player)
        if exe:
            args = [exe, "-q", str(path)] if player == "aplay" else [exe, str(path)]
            try:
                subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
                return True
            except OSError:
                continue
    return False
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
_TR = {"Fermer": ("Close", "Cerrar", "Schließen"), "Notification": ("Notification", "Notificación", "Benachrichtigung")}


def _t(text):
    row = _TR.get(text)
    return row[("en", "es", "de").index(_LANG)] if row and _LANG != "fr" else text


XML = '''<node><interface name="org.freedesktop.Notifications">
<method name="GetCapabilities"><arg type="as" direction="out"/></method>
<method name="GetServerInformation"><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/></method>
<method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
<method name="CloseNotification"><arg type="u" direction="in"/></method>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
</interface><interface name="org.winlux.Notifications">
<method name="Clear"/><method name="Dismiss"><arg type="u" direction="in"/></method><method name="Activate"><arg type="u" direction="in"/><arg type="s" direction="in"/></method>
</interface></node>'''

class Notifications:
    def __init__(self):
        try: self.history = json.loads(STATE.read_text())[-200:]
        except (OSError, ValueError): self.history = []
        self.sequence = max([item.get("id", 0) for item in self.history] + [0])
        self.live = {}
        self.toast = None
        # Clé USB, disque ou lecteur branché / retiré : sons système assortis.
        try:
            self.volumes = Gio.VolumeMonitor.get()
            self.volumes.connect("drive-connected", lambda *_: play_sound("appareil-branche"))
            self.volumes.connect("drive-disconnected", lambda *_: play_sound("appareil-retire"))
        except Exception:
            self.volumes = None
        self.owner = Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.Notifications",
            Gio.BusNameOwnerFlags.NONE, self.acquired, None, lambda *_: Gtk.main_quit())

    def save(self):
        STATE.parent.mkdir(parents=True, exist_ok=True)
        tmp = STATE.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.history[-200:], ensure_ascii=False))
        tmp.chmod(0o600)
        tmp.replace(STATE)

    def acquired(self, connection, _):
        self.bus = connection
        for interface in Gio.DBusNodeInfo.new_for_xml(XML).interfaces:
            connection.register_object("/org/freedesktop/Notifications", interface, self.method, None, None)

    def method(self, connection, sender, path, interface, method, params, invocation):
        values = params.unpack()
        if method == "GetCapabilities": invocation.return_value(GLib.Variant("(as)", (["body", "actions", "persistence"],)))
        elif method == "GetServerInformation": invocation.return_value(GLib.Variant("(ssss)", ("Winlux", "Winlux", "1.0", "1.2")))
        elif method == "Clear":
            for identity,(origin,_) in self.live.items():
                connection.emit_signal(origin,path,"org.freedesktop.Notifications","NotificationClosed",GLib.Variant("(uu)",(identity,2)))
            self.live.clear()
            if self.toast:
                self.toast.destroy()
                self.toast=None
            self.history = []
            self.save()
            invocation.return_value(None)
        elif method == "Dismiss":
            # Croix d'une notification dans le panneau : retirée de l'historique.
            identity = values[0]
            live = self.live.pop(identity, None)
            if live:
                connection.emit_signal(live[0], path, "org.freedesktop.Notifications", "NotificationClosed", GLib.Variant("(uu)", (identity, 2)))
            if self.toast and getattr(self, "toast_id", None) == identity:
                self.toast.destroy(); self.toast = None
            self.history = [item for item in self.history if item["id"] != identity]
            self.save()
            invocation.return_value(None)
        elif method == "Activate":
            identity, action = values
            live = self.live.get(identity)
            if live and action in live[1][::2]:
                connection.emit_signal(live[0], path, "org.freedesktop.Notifications", "ActionInvoked", GLib.Variant("(us)", (identity, action)))
            else:
                invocation.return_dbus_error("org.winlux.Notifications.Expired","Cette action n’est plus disponible dans la session courante.")
                return
            invocation.return_value(None)
        elif method == "CloseNotification":
            identity = values[0]
            if identity in self.live:
                self.live.pop(identity)
                if self.toast and getattr(self,"toast_id",None)==identity:
                    self.toast.destroy();self.toast=None
                connection.emit_signal(None, path, interface, "NotificationClosed", GLib.Variant("(uu)", (identity, 3)))
            invocation.return_value(None)
        elif method == "Notify":
            app, replace, icon, title, body, actions, hints, timeout = values
            if replace in self.live: identity = replace
            else:
                self.sequence = self.sequence % 4294967295 + 1
                identity = self.sequence
            self.live[identity] = (sender, actions[:32])
            self.history = [item for item in self.history if item["id"] != identity]
            self.history.append(dict(id=identity, app=app[:256], title=title[:1024], body=body[:8192],
                                     actions=actions[:32], time=int(time.time())))
            self.history = self.history[-200:]
            retained = {item["id"] for item in self.history}
            self.live = {key: value for key, value in self.live.items() if key in retained}
            self.save()
            if not PREF.exists():
                self.show(identity,title[:1024],body[:8192],timeout,app=app[:256],icon=icon,hints=hints)
                # Indices standard : « suppress-sound » coupe le son,
                # « sound-name » choisit un son système Winlux connu.
                if not bool(hints.get("suppress-sound", False)):
                    wanted = str(hints.get("sound-name", "notification"))
                    play_sound(wanted if (SOUNDS / f"{wanted}.wav").is_file() and "/" not in wanted else "notification")
            invocation.return_value(GLib.Variant("(u)", (identity,)))

    def show(self, identity, title, body, timeout, app="", icon="", hints=None):
        """Bulle façon Windows 11 (#624) : en bas à droite, juste au-dessus de la
        barre des tâches (zone de travail), jamais sur les boutons d'une fenêtre
        agrandie ; en-tête icône + nom de l'application + croix, puis titre en
        gras et texte, alignés à gauche. Un clic sur la bulle lance l'action
        « default » si l'application en propose une."""
        if self.toast: self.toast.destroy()
        hints = hints or {}
        name, app_icon = app_identity(app, icon, hints)
        popup = Gtk.Window(type=Gtk.WindowType.POPUP)
        popup.set_name("winlux-toast")
        popup.set_title(title or name)
        popup.set_keep_above(True)
        popup.set_accept_focus(False)
        popup.set_type_hint(Gdk.WindowTypeHint.NOTIFICATION)
        popup.set_size_request(TOAST_WIDTH, -1)
        events = Gtk.EventBox()
        popup.add(events)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=14)
        events.add(box)
        header = Gtk.Box(spacing=8)
        header.pack_start(Gtk.Image.new_from_gicon(app_icon, Gtk.IconSize.MENU), False, False, 0)
        source = Gtk.Label(label=name, xalign=0, ellipsize=3)
        source.get_style_context().add_class("dim-label")
        header.pack_start(source, True, True, 0)
        close = Gtk.Button.new_from_icon_name("window-close-symbolic", Gtk.IconSize.MENU)
        close.set_relief(Gtk.ReliefStyle.NONE)
        close.set_tooltip_text(_t("Fermer"))
        header.pack_end(close, False, False, 0)
        box.pack_start(header, False, False, 0)
        content = Gtk.Box(spacing=12)
        picture = hint_image(hints)
        if picture is not None:
            content.pack_start(picture, False, False, 0)
        texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        heading = Gtk.Label(label=title, xalign=0, wrap=True, max_width_chars=38)
        heading.get_style_context().add_class("toast-title")
        texts.pack_start(heading, False, False, 0)
        if body:
            texts.pack_start(Gtk.Label(label=body, xalign=0, wrap=True, max_width_chars=38, lines=4, ellipsize=3), False, False, 0)
        content.pack_start(texts, True, True, 0)
        box.pack_start(content, False, False, 0)
        css = Gtk.CssProvider()
        css.load_from_data(b"#winlux-toast { border: 1px solid alpha(currentColor, 0.18); border-radius: 8px; }"
                           b"#winlux-toast .toast-title { font-weight: bold; }")
        popup.get_style_context().add_provider(css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        heading.get_style_context().add_provider(css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        box.show_all(); events.show(); popup.realize()
        # Placée avant d'apparaître : aucun saut depuis le coin supérieur gauche.
        height = popup.get_preferred_height()[1]
        display = popup.get_display()
        monitor = display.get_primary_monitor() or display.get_monitor(0)
        x, y = toast_position(monitor.get_workarea(), monitor.get_geometry(), TOAST_WIDTH, height)
        popup.move(x, y)
        popup.show_all()
        self.toast = popup
        self.toast_id=identity
        def hide(*_):
            if self.toast == popup:
                popup.destroy()
                self.toast = None
            return False
        close.connect("clicked", hide)
        def clicked(_, event):
            live = self.live.get(identity)
            if event.button == 1 and live and "default" in live[1][::2] and getattr(self, "bus", None):
                self.bus.emit_signal(live[0], "/org/freedesktop/Notifications", "org.freedesktop.Notifications",
                                     "ActionInvoked", GLib.Variant("(us)", (identity, "default")))
            return hide()
        events.connect("button-release-event", clicked)
        if timeout!=0:GLib.timeout_add(max(500,min(timeout if timeout>0 else 7000,60000)),hide)


TOAST_WIDTH = 364
TOAST_MARGIN = 12


def toast_position(workarea, geometry, width, height):
    """Coin inférieur droit de la zone de travail (au-dessus de la barre des
    tâches). Sans réservation de la barre (zone de travail = écran entier),
    48 px sont laissés en bas pour ne pas la recouvrir."""
    bottom = workarea.y + workarea.height
    if bottom >= geometry.y + geometry.height:
        bottom = geometry.y + geometry.height - 48
    right = workarea.x + workarea.width
    return right - width - TOAST_MARGIN, bottom - height - TOAST_MARGIN


def app_identity(app, icon, hints):
    """Nom et icône de l'application : lanceur « desktop-entry », sinon le nom
    et l'icône envoyés, sinon une cloche."""
    name, gicon = app or "", None
    entry = str(hints.get("desktop-entry", "") or "")
    if entry and "/" not in entry:
        try:
            info = Gio.DesktopAppInfo.new(entry if entry.endswith(".desktop") else entry + ".desktop")
        except TypeError:
            info = None
        if info is not None:
            name = info.get_display_name() or name
            gicon = info.get_icon()
    if icon and gicon is None:
        gicon = file_icon(icon) or (Gio.ThemedIcon.new(icon) if "/" not in icon else None)
    return name or _t("Notification"), gicon or Gio.ThemedIcon.new("preferences-system-notifications-symbolic")


def file_icon(value):
    path = value[7:] if value.startswith("file://") else value
    if path.startswith("/") and os.path.isfile(path):
        return Gio.FileIcon.new(Gio.File.new_for_path(path))
    return None


def hint_image(hints):
    """Image jointe (« image-path ») : vignette de 48 px à gauche du texte."""
    value = str(hints.get("image-path", "") or hints.get("image_path", "") or "")
    gicon = file_icon(value) if value else None
    if gicon is None and value and "/" not in value: gicon = Gio.ThemedIcon.new(value)
    if gicon is None: return None
    image = Gtk.Image.new_from_gicon(gicon, Gtk.IconSize.DIALOG)
    image.set_pixel_size(48)
    image.set_valign(Gtk.Align.START)
    return image

if __name__ == "__main__":
    Notifications()
    Gtk.main()
