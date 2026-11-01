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
from gi.repository import Gtk, Gio, GLib

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
                self.show(identity,title[:1024],body[:8192],timeout)
                # Indices standard : « suppress-sound » coupe le son,
                # « sound-name » choisit un son système Winlux connu.
                if not bool(hints.get("suppress-sound", False)):
                    wanted = str(hints.get("sound-name", "notification"))
                    play_sound(wanted if (SOUNDS / f"{wanted}.wav").is_file() and "/" not in wanted else "notification")
            invocation.return_value(GLib.Variant("(u)", (identity,)))

    def show(self, identity, title, body, timeout):
        if self.toast: self.toast.destroy()
        popup = Gtk.Window(type=Gtk.WindowType.POPUP)
        popup.set_keep_above(True)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=18)
        popup.add(box)
        box.pack_start(Gtk.Label(label=title, wrap=True, max_width_chars=45), False, False, 0)
        box.pack_start(Gtk.Label(label=body, wrap=True, max_width_chars=45), False, False, 0)
        popup.show_all()
        area = popup.get_display().get_monitor(0).get_workarea()
        popup.move(area.x + area.width - popup.get_allocated_width() - 20, area.y + 20)
        self.toast = popup
        self.toast_id=identity
        def hide():
            if self.toast == popup:
                popup.destroy()
                self.toast = None
            return False
        if timeout!=0:GLib.timeout_add(max(500,min(timeout if timeout>0 else 7000,60000)),hide)

if __name__ == "__main__":
    Notifications()
    Gtk.main()
