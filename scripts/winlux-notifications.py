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
# Nombre de notifications nouvelles (#3057) : arrivées sans que l'utilisateur
# les ait vues, ni sur la bulle, ni dans le centre. La pastille de la cloche
# lit ce petit fichier au lieu de l'historique entier.
UNSEEN = STATE.with_name("notifications-nouvelles")
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
<method name="Clear"/><method name="MarkSeen"/><method name="Dismiss"><arg type="u" direction="in"/></method><method name="Activate"><arg type="u" direction="in"/><arg type="s" direction="in"/></method>
</interface></node>'''

OBJECT_PATH = "/org/freedesktop/Notifications"
INTROSPECTABLE = "org.freedesktop.DBus.Introspectable"
# Signature d'entrée de chaque méthode servie, tirée de XML.
METHODS = {interface.name: {method.name: "".join(arg.signature for arg in method.in_args)
                            for method in interface.methods}
           for interface in Gio.DBusNodeInfo.new_for_xml(XML).interfaces}
# Membres servis quand l'appel ne nomme pas d'interface (permis par D-Bus).
OWN_MEMBERS = {name: interface for interface, methods in METHODS.items() for name in methods}
OWN_MEMBERS["Introspect"] = INTROSPECTABLE


# Caractères du corps donnés à la bulle (4 lignes de 38 au plus affichées).
TOAST_BODY = 600


class BusReply:
    """Réponse à un appel reçu par le filtre (#3278) : mêmes noms que
    Gio.DBusMethodInvocation, sans en garder aucune."""
    __slots__ = ("connection", "message")

    def __init__(self, connection, message):
        self.connection, self.message = connection, message

    def send(self, reply):
        if not self.message.get_flags() & Gio.DBusMessageFlags.NO_REPLY_EXPECTED:
            self.connection.send_message(reply, Gio.DBusSendMessageFlags.NONE)

    def return_value(self, value):
        reply = self.message.new_method_reply()
        if value is not None:
            reply.set_body(value)
        self.send(reply)

    def return_dbus_error(self, name, text):
        self.send(self.message.new_method_error_literal(name, text))


class Notifications:
    def __init__(self):
        # Temporaire d'avant #3060, né en 0644 : jamais laissé derrière.
        try: STATE.with_suffix(".tmp").unlink()
        except OSError: pass
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
        if getattr(self, "saving", None):
            GLib.source_remove(self.saving); self.saving = None
        private_state()
        write_private(STATE, json.dumps(self.history[-200:], ensure_ascii=False))
        write_unseen(unseen_count(self.history))

    def save_later(self):
        """Historique écrit une fois la file d'appels vidée (#3343) : une rafale
        de notifications donne une écriture, pas une par notification."""
        if not getattr(self, "saving", None):
            self.saving = GLib.idle_add(self.save_pending, priority=GLib.PRIORITY_LOW)

    def save_pending(self):
        self.saving = None
        self.save()
        return False

    def mark_seen(self, identity=None):
        """Notification vue (bulle cliquée ou fermée), ou toutes (centre ouvert)."""
        changed = False
        # Bulle seule, sans historique chargé (test de la bulle) : rien à marquer.
        for item in getattr(self, "history", ()):
            if item.get("nouvelle") and identity in (None, item.get("id")):
                item.pop("nouvelle"); changed = True
        if changed: self.save()

    def acquired(self, connection, _):
        self.bus = connection
        # Pas de register_object (#3278) : chez PyGObject, chaque appel servi
        # par une fonction Python garde son GDBusMethodInvocation (message et
        # paramètres) à jamais, environ 3,6 Ko par notification sur l'ISO
        # (GLib 2.88, PyGObject 3.56). Le filtre de la connexion reçoit le
        # message lui-même, et la réponse est construite à partir de lui.
        connection.add_filter(self.incoming)

    def incoming(self, connection, message, received):
        """Filtre de la connexion, dans le fil de GDBus : les appels adressés
        à l'objet des notifications sont servis dans la boucle principale ;
        tout le reste suit son chemin normal."""
        if (received and message.get_message_type() == Gio.DBusMessageType.METHOD_CALL
                and message.get_path() == OBJECT_PATH
                and (message.get_interface() in (*METHODS, INTROSPECTABLE)
                     or message.get_interface() is None and message.get_member() in OWN_MEMBERS)):
            GLib.idle_add(self.dispatch, connection, message, priority=GLib.PRIORITY_DEFAULT)
            return None
        return message

    def dispatch(self, connection, message):
        interface, member = message.get_interface(), message.get_member()
        reply = BusReply(connection, message)
        if interface is None:
            interface = OWN_MEMBERS[member]
        if interface == INTROSPECTABLE:
            if member == "Introspect" and message.get_signature() == "":
                reply.return_value(GLib.Variant("(s)", (XML,)))
            else:
                reply.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", f"Méthode inconnue : {member}")
            return False
        signature = METHODS.get(interface, {}).get(member)
        if signature is None:
            reply.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", f"Méthode inconnue : {interface}.{member}")
        elif message.get_signature() != signature:
            reply.return_dbus_error("org.freedesktop.DBus.Error.InvalidArgs",
                                    f"Signature « {message.get_signature()} » au lieu de « {signature} »")
        else:
            self.method(connection, message.get_sender(), OBJECT_PATH, interface, member,
                        message.get_body() or GLib.Variant("()", ()), reply)
        return False

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
        elif method == "MarkSeen":
            self.mark_seen()
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
                                     actions=actions[:32], time=int(time.time()), nouvelle=True))
            self.history = self.history[-200:]
            retained = {item["id"] for item in self.history}
            self.live = {key: value for key, value in self.live.items() if key in retained}
            # Réponse d'abord (#3343) : le programme qui appelle (libnotify
            # attend la réponse) ne patiente ni pour le disque ni pour la bulle.
            invocation.return_value(GLib.Variant("(u)", (identity,)))
            self.save_later()
            if not PREF.exists():
                # La bulle n'en montre que 4 lignes : inutile de mettre en page
                # 8 192 caractères (50 ms) pour les cacher.
                self.show(identity,title[:1024],body[:TOAST_BODY],timeout,app=app[:256],icon=icon,hints=hints)
                # Indices standard : « suppress-sound » coupe le son,
                # « sound-name » choisit un son système Winlux connu.
                if not bool(hints.get("suppress-sound", False)):
                    wanted = str(hints.get("sound-name", "notification"))
                    play_sound(wanted if (SOUNDS / f"{wanted}.wav").is_file() and "/" not in wanted else "notification")

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
        def seen(*_):
            # Bulle fermée ou cliquée : vue. Bulle expirée : reste nouvelle.
            # La bulle se ferme d'abord : un disque plein ne la laisse pas affichée.
            hidden = hide()
            try: self.mark_seen(identity)
            except OSError: pass
            return hidden
        close.connect("clicked", seen)
        def clicked(_, event):
            live = self.live.get(identity)
            if event.button == 1 and live and "default" in live[1][::2] and getattr(self, "bus", None):
                self.bus.emit_signal(live[0], "/org/freedesktop/Notifications", "org.freedesktop.Notifications",
                                     "ActionInvoked", GLib.Variant("(us)", (identity, "default")))
            return seen()
        events.connect("button-release-event", clicked)
        if timeout!=0:GLib.timeout_add(max(500,min(timeout if timeout>0 else 7000,60000)),hide)


def unseen_count(history):
    return sum(1 for item in history if isinstance(item, dict) and item.get("nouvelle"))


def private_state():
    """Dossier d'état du compte en 0700 (#3060), comme private_state de
    winlux-open : créé sous umask 077 (~/.local/state compris quand il
    manque), et notre dossier winlux ramené à 0700 s'il existait plus ouvert."""
    old = os.umask(0o077)
    try:
        STATE.parent.mkdir(parents=True, exist_ok=True)
    finally:
        os.umask(old)
    if STATE.parent.stat().st_mode & 0o077:
        STATE.parent.chmod(0o700)


def write_private(path, text):
    """Fichier né en 0600 (#3060) : jamais lisible par un autre compte, pas
    même entre l'écriture et un chmod. Remplacé d'un bloc."""
    tmp = path.with_name(f"{path.name}.{os.getpid()}.tmp")
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as out: out.write(text)
        tmp.replace(path)
    except BaseException:
        tmp.unlink(missing_ok=True)
        raise


def write_unseen(count):
    """Compteur privé au compte, remplacé d'un bloc (jamais lu à moitié)."""
    try:
        write_private(UNSEEN, f"{count}\n")
    except OSError:
        pass


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
