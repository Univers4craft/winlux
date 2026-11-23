#!/usr/bin/python3
"""org.freedesktop.ScreenSaver sur le bus de session : vidéo et jeux sans économiseur.

Les navigateurs, lecteurs vidéo et jeux SDL demandent « pas d'économiseur »
par org.freedesktop.ScreenSaver.Inhibit. Sous XScreenSaver, c'est
xscreensaver-systemd qui répond, mais il s'arrête sans logind (« inhibit sleep
failed ») : sous Winlux, sans systemd, personne ne répondait et l'économiseur
coupait une vidéo au bout de 10 min. Ce service, lancé à la demande par
D-Bus, tient la liste des demandes ; tant qu'il en reste une, il réveille
XScreenSaver et le compteur d'inactivité de X, et pose une demande d'éveil au
démon d'alimentation Winlux. Une demande disparaît avec le programme qui l'a
faite.

L'écran s'éteignait quand même (#634) : XScreenSaver n'est pas seul, le
serveur X coupe l'écran par DPMS à son propre délai (Standby/Suspend/Off à
600 s). Tant qu'une demande est là, le DPMS est donc suspendu (`xset -dpms`,
réaffirmé à chaque réveil au cas où XScreenSaver le remettrait) puis rétabli
s'il était actif, dès que la dernière demande tombe, y compris quand le service
est arrêté par un signal ; une marque dans $XDG_RUNTIME_DIR le rétablit aussi
après un arrêt brutal (#735).

  winlux-screensaver-inhibit                  service D-Bus (activé à la demande)
  winlux-screensaver-inhibit watch-updates    lancé avec la session : pendant
      une installation de mise à jour (`winlux-update --busy`, automatique
      comprise, Paramètres fermé), une demande est tenue : l'écran ne s'éteint
      pas en pleine installation (#634). Il remet aussi le DPMS que
      `xscreensaver-auth --init` coupe à chaque démarrage d'XScreenSaver (#749).

Journal du service activé par D-Bus : screensaver-inhibit.log dans les
journaux de session ($WINLUX_SESSION_LOGS ou $XDG_RUNTIME_DIR/winlux, #750).
"""
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import time

NAME = "org.freedesktop.ScreenSaver"
PATHS = ("/org/freedesktop/ScreenSaver", "/ScreenSaver")
XML = f'''<node><interface name="{NAME}">
<method name="Inhibit"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="u" direction="out"/></method>
<method name="UnInhibit"><arg type="u" direction="in"/></method>
<method name="SimulateUserActivity"/>
<method name="GetActive"><arg type="b" direction="out"/></method>
<method name="GetActiveTime"><arg type="u" direction="out"/></method>
<method name="SetActive"><arg type="b" direction="in"/><arg type="b" direction="out"/></method>
</interface></node>'''
INTROSPECTABLE = "org.freedesktop.DBus.Introspectable"
POKE_SECONDS = 50
INHIBIT_DIR = Path(os.environ.get("WINLUX_POWER_INHIBIT_DIR", "/run/winlux/power-inhibit"))
IDLE_EXIT_SECONDS = 600


class BusReply:
    """Réponse à un appel reçu par le filtre de la connexion (#3341) : mêmes
    noms que Gio.DBusMethodInvocation, dont PyGObject garde chaque instance
    servie par register_object (environ 2 Ko par appel, sans plafond)."""
    __slots__ = ("connection", "message")

    def __init__(self, connection, message):
        self.connection, self.message = connection, message

    def send(self, reply):
        from gi.repository import Gio
        if not self.message.get_flags() & Gio.DBusMessageFlags.NO_REPLY_EXPECTED:
            self.connection.send_message(reply, Gio.DBusSendMessageFlags.NONE)

    def return_value(self, value):
        reply = self.message.new_method_reply()
        if value is not None:
            reply.set_body(value)
        self.send(reply)

    def return_dbus_error(self, name, text):
        self.send(self.message.new_method_error_literal(name, text))


def bus_filter(methods, call):
    """Filtre de connexion qui sert les appels de NAME sur PATHS dans la boucle
    principale (#3341) ; tout autre message suit son chemin normal.
    methods : nom → signature d'entrée."""
    from gi.repository import Gio, GLib

    def dispatch(connection, message):
        interface, member = message.get_interface(), message.get_member()
        reply = BusReply(connection, message)
        if interface is None:
            interface = INTROSPECTABLE if member == "Introspect" else NAME
        if interface == INTROSPECTABLE:
            if member == "Introspect" and message.get_signature() == "":
                reply.return_value(GLib.Variant("(s)", (XML,)))
            else:
                reply.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", f"Méthode inconnue : {member}")
            return False
        signature = methods.get(member)
        if signature is None:
            reply.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", f"Méthode inconnue : {interface}.{member}")
        elif message.get_signature() != signature:
            reply.return_dbus_error("org.freedesktop.DBus.Error.InvalidArgs",
                                    f"Signature « {message.get_signature()} » au lieu de « {signature} »")
        else:
            call(connection, message.get_sender(), message.get_path(), interface, member,
                 message.get_body() or GLib.Variant("()", ()), reply)
        return False

    def incoming(connection, message, received):
        interface, member = message.get_interface(), message.get_member()
        if (received and message.get_message_type() == Gio.DBusMessageType.METHOD_CALL
                and message.get_path() in PATHS
                and (interface in (NAME, INTROSPECTABLE)
                     or interface is None and (member in methods or member == "Introspect"))):
            GLib.idle_add(dispatch, connection, message, priority=GLib.PRIORITY_DEFAULT)
            return None
        return message
    return incoming


def log(message, error=False):
    stamp = time.strftime("%H:%M:%S")
    print(f"{stamp} {message}", file=sys.stderr if error else sys.stdout, flush=True)


def session_log_dir():
    logs = os.environ.get("WINLUX_SESSION_LOGS")
    if logs: return Path(logs)
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    return Path(runtime) / "winlux" if runtime else None


def own_log(name="screensaver-inhibit.log", directory=None, limit=512 * 1024):
    """#750 : activé par D-Bus, le service hérite des fd 1 et 2 de dbus-daemon
    (--fork : /dev/null) et son journal partait dans le vide. Sortie vers
    /dev/null : redirigée vers son propre fichier dans les journaux de session."""
    try:
        null = os.stat(os.devnull).st_rdev
        out = os.fstat(1)
        if not (stat.S_ISCHR(out.st_mode) and out.st_rdev == null): return None
    except OSError:
        pass   # fd 1 fermé : redirigé aussi
    directory = directory or session_log_dir()
    if directory is None: return None
    path = Path(directory) / name
    try:
        directory = Path(directory)
        directory.mkdir(parents=True, exist_ok=True)
        if path.exists() and path.stat().st_size > limit: path.replace(path.with_name(name + ".1"))
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600)
    except OSError:
        return None
    sys.stdout.flush(); sys.stderr.flush()
    os.dup2(fd, 1); os.dup2(fd, 2); os.close(fd)
    return path


class Inhibitors:
    """Demandes en cours : cookie → (programme D-Bus, application, raison)."""

    def __init__(self, poke=None, inhibit_file=None, hold=None):
        self.items = {}
        self.hold = hold if hold is not None else DisplayHold()
        self.next_cookie = 1
        self.poke = poke or wake_screen
        self.inhibit_file = inhibit_file or INHIBIT_DIR / f"{os.getpid()}-screensaver"

    def add(self, sender, application, reason):
        cookie = self.next_cookie
        self.next_cookie += 1
        self.items[cookie] = (sender, application, reason)
        if len(self.items) == 1:
            self.poke()
        self.sync()
        return cookie

    def remove(self, cookie):
        self.items.pop(cookie, None)
        self.sync()

    def forget(self, sender):
        """Le programme a quitté le bus : ses demandes tombent avec lui."""
        for cookie in [c for c, item in self.items.items() if item[0] == sender]:
            del self.items[cookie]
        self.sync()

    def senders(self):
        return {item[0] for item in self.items.values()}

    def sync(self):
        """Demande d'éveil au démon d'alimentation (écran et veille) tant qu'il en
        reste, et DPMS du serveur X suspendu pendant ce temps."""
        if self.items: self.hold.start()
        else: self.hold.stop()
        try:
            if self.items:
                self.inhibit_file.parent.mkdir(parents=True, exist_ok=True)
                self.inhibit_file.write_text("0x80000003\n")   # ES_CONTINUOUS | SYSTEM | DISPLAY
            else:
                self.inhibit_file.unlink(missing_ok=True)
        except OSError as error:
            log(f"Demande d'éveil non posée ({self.inhibit_file}) : {error}", error=True)


def _xset(*args):
    if not shutil.which("xset"): return ""
    try:
        return subprocess.run(["xset", *args], capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


class DisplayHold:
    """DPMS du serveur X suspendu tant qu'une demande est posée.

    #735 : le retour du DPMS ne dépendait que de la marque sur disque. Marque
    perdue (effacée par une autre instance, dossier nettoyé) : plus de
    `xset +dpms` à la libération, l'écran ne s'éteignait plus de la session.
    Ce que ce service a coupé est donc retenu en mémoire, la marque porte le
    numéro du service qui l'a posée (une autre instance n'y touche pas tant
    qu'il vit) et le retour est vérifié par `xset q`."""

    def __init__(self, xset=_xset, marker=None):
        self.xset = xset
        runtime = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
        display = "".join(c if c.isalnum() else "_" for c in os.environ.get("DISPLAY", "none"))
        self.marker = Path(marker) if marker else Path(runtime) / f"winlux-dpms-suspendu-{display}"
        self.active = False
        self.suspended = False   # ce service a coupé le DPMS : à lui de le rendre
        self.pid = os.getpid()

    def owner(self):
        """Numéro du service qui a posé la marque (None : ancienne marque ou illisible)."""
        try: text = self.marker.read_text().strip()
        except OSError: return None
        return int(text) if text.isdigit() and int(text) > 1 else None

    def held_by_other(self):
        """Une demande en cours dans le service (marque d'un processus vivant)."""
        owner = self.owner()
        return bool(owner and owner != self.pid and os.path.isdir(f"/proc/{owner}"))

    def recover(self):
        """Service arrêté brutalement pendant une demande : DPMS rétabli. Une
        marque posée par un service encore vivant reste à lui."""
        if self.active or not self.marker.exists(): return
        owner = self.owner()
        if owner and owner != self.pid and os.path.isdir(f"/proc/{owner}"): return
        self.restore()

    def start(self):
        if not self.active:
            self.active = True
            if "DPMS is Enabled" in self.xset("q"):
                self.suspended = True
                log("Veille de l'écran suspendue (DPMS)")
                try: self.marker.write_text(f"{self.pid}\n")
                except OSError: pass
        self.reassert()

    def reassert(self):
        if self.active and self.suspended:
            self.xset("-dpms")

    def stop(self):
        if not self.active: return
        self.active = False
        if self.suspended or (self.marker.exists() and self.owner() == self.pid):
            self.suspended = False
            self.restore()

    def restore(self):
        """`xset +dpms`, vérifié (un second essai si le serveur ne l'a pas pris)."""
        for _ in range(2):
            self.xset("+dpms")
            if "DPMS is Disabled" not in self.xset("q"): break
        log("Veille de l'écran rétablie (DPMS)")
        try: self.marker.unlink(missing_ok=True)
        except OSError: pass


def wake_screen():
    for command in (["xscreensaver-command", "-deactivate"], ["xset", "s", "reset"]):
        if shutil.which(command[0]):
            try:
                subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
            except (OSError, subprocess.SubprocessError):
                pass


class UpdateWatch:
    """Une demande tenue tant qu'une installation est en cours."""

    def __init__(self, busy, inhibit, uninhibit):
        self.busy, self.inhibit, self.uninhibit = busy, inhibit, uninhibit
        self.cookie = None

    def tick(self):
        busy = self.busy()
        try:
            if busy and self.cookie is None:
                self.cookie = self.inhibit()
                log(f"Mise à jour en cours : écran gardé allumé ({self.cookie})")
            elif not busy and self.cookie is not None:
                cookie, self.cookie = self.cookie, None
                self.uninhibit(cookie)
                log("Mise à jour terminée : écran rendu")
        except Exception as error:   # bus absent, service relancé : réessayé au tour suivant
            log(f"Demande « écran allumé » impossible : {error}", error=True)
        return True


def saver_processes(proc="/proc", uid=None):
    """(PID des démons XScreenSaver du compte, xscreensaver-auth --init en cours)."""
    uid = os.getuid() if uid is None else uid
    daemons, init = set(), False
    try: entries = os.listdir(proc)
    except OSError: return daemons, init
    for entry in entries:
        if not entry.isdigit(): continue
        try:
            argv = Path(proc, entry, "cmdline").read_bytes().split(b"\0")
            owner = os.stat(Path(proc, entry)).st_uid
        except OSError:
            continue
        name = os.path.basename(argv[0].decode(errors="replace")) if argv and argv[0] else ""
        if name == "xscreensaver" and owner == uid: daemons.add(int(entry))
        elif name == "xscreensaver-auth" and b"--init" in argv: init = True
    return daemons, init


def saver_alive(pid, proc="/proc"):
    """Le démon XScreenSaver déjà connu tourne-t-il encore ? Un seul fichier
    lu, au lieu du cmdline de tous les processus (#3291)."""
    try: argv0 = Path(proc, str(pid), "cmdline").read_bytes().split(b"\0")[0]
    except OSError: return False
    return os.path.basename(argv0.decode(errors="replace")) == "xscreensaver"


def dpms_wanted(path=None):
    """Choix de l'utilisateur dans ~/.xscreensaver (Winlux : actif par défaut)."""
    path = Path(path) if path else Path.home() / ".xscreensaver"
    try: lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError: return True
    for line in lines:
        key, _, value = line.partition(":")
        if key.strip() == "dpmsEnabled": return value.strip().lower() not in ("false", "off", "no", "0")
    return True


class SaverDpms:
    """#749 : à chaque démarrage, XScreenSaver lance `xscreensaver-auth --init`
    qui coupe TOUJOURS le DPMS (il ne le rend qu'au premier économiseur) : l'écran
    ne s'éteignait plus. Pendant la minute qui suit l'apparition d'un démon
    XScreenSaver, une fois --init terminé, le DPMS est remis, sauf demande
    « écran allumé » en cours ou choix « dpmsEnabled: False »."""

    WINDOW = 60

    def __init__(self, xset=_xset, processes=saver_processes, wanted=dpms_wanted, held=None, clock=time.monotonic,
                 alive=saver_alive):
        self.xset, self.processes, self.wanted, self.clock, self.alive = xset, processes, wanted, clock, alive
        self.held = held or (lambda: DisplayHold(xset=lambda *_: "").held_by_other())
        self.daemon = None
        self.until = 0

    def tick(self):
        # Hors de la minute qui suit son apparition, le démon connu n'appelle
        # plus rien : on vérifie seulement qu'il vit encore. Le parcours de
        # tout /proc ne revient que s'il a disparu (relance d'XScreenSaver).
        if self.daemon is not None and self.clock() > self.until and self.alive(self.daemon):
            return True
        daemons, init = self.processes()
        if not daemons:
            self.daemon = None
            return True
        current = min(daemons)
        if current != self.daemon:
            self.daemon, self.until = current, self.clock() + self.WINDOW
        if self.clock() > self.until or init: return True
        if not self.wanted() or self.held(): return True
        if "DPMS is Disabled" in self.xset("q"):
            self.xset("+dpms")
            log(f"XScreenSaver ({current}) avait coupé la veille de l'écran : DPMS rétabli")
        return True


def update_busy(root="/"):
    """Même réponse que `winlux-update --busy` (branche du moteur APT, à garder
    synchronisée : scripts/test-veille-repos.py rejoue cette branche du shell
    sur les mêmes cas et échoue si les deux réponses divergent), sans lancer de programme
    (#3291 : 30 ms de CPU et une dizaine de processus toutes les 15 s).
    Installation en cours : drapeau de root /run/winlux/update-inhibit dont le
    processus vit, ou verrou POSIX d'APT/dpkg posé (/proc/locks)."""
    flag = Path(root, "run/winlux/update-inhibit")
    try:
        info = os.lstat(flag)
        if stat.S_ISREG(info.st_mode) and info.st_uid == 0:
            pid = flag.read_text(errors="replace").rstrip("\n")   # comme $(cat …)
            if pid.isascii() and pid.isdigit() and pid != "0" and Path(root, "proc", pid).is_dir():
                return True
    except OSError:
        pass
    inodes = set()
    for name in ("lock-frontend", "lock"):
        try: inodes.add(os.stat(Path(root, "var/lib/dpkg", name)).st_ino)
        except OSError: pass
    if not inodes: return False
    try: locks = Path(root, "proc/locks").read_text(errors="replace").splitlines()
    except OSError: return False
    for line in locks:
        # « 1: POSIX  ADVISORY  WRITE 1234 08:02:131 0 EOF », ou « 1: -> POSIX … »
        # pour une attente : même lecture que `grep " POSIX .*:$inode "`.
        found = re.search(r" POSIX .*:(\d+) ", line)
        if found and int(found.group(1)) in inodes: return True
    return False


def watch_updates(interval=None):
    from gi.repository import Gio, GLib
    runtime = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    display = "".join(c if c.isalnum() else "_" for c in os.environ.get("DISPLAY", "none"))
    lock = Path(runtime) / f"winlux-update-screen-{display}.pid"
    try:
        if os.path.isdir(f"/proc/{int(lock.read_text().strip())}"): return 0
    except (OSError, ValueError):
        pass
    try: lock.write_text(f"{os.getpid()}\n")
    except OSError: pass
    saver = SaverDpms()
    saver.tick()
    GLib.timeout_add_seconds(int(os.environ.get("WINLUX_SAVER_DPMS_SECONDS", "3")), saver.tick)
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

    def inhibit():
        reply = bus.call_sync(NAME, PATHS[0], NAME, "Inhibit", GLib.Variant("(ss)", ("Winlux", "Installation des mises à jour")),
                              GLib.VariantType("(u)"), Gio.DBusCallFlags.NONE, 10000, None)
        return reply.unpack()[0]

    def uninhibit(cookie):
        bus.call_sync(NAME, PATHS[0], NAME, "UnInhibit", GLib.Variant("(u)", (cookie,)), None, Gio.DBusCallFlags.NONE, 10000, None)

    # WINLUX_UPDATE_ROOT : racine lue par update_busy, pour les tests qui
    # simulent une installation sans être root (test-boot-journaux.py).
    root = os.environ.get("WINLUX_UPDATE_ROOT", "/")
    watcher = UpdateWatch(lambda: update_busy(root), inhibit, uninhibit)
    watcher.tick()
    GLib.timeout_add_seconds(interval or int(os.environ.get("WINLUX_UPDATE_WATCH_SECONDS", "15")), watcher.tick)
    GLib.MainLoop().run()
    return 0


def main():
    if sys.argv[1:] == ["watch-updates"]: return watch_updates()
    import signal
    own_log()
    from gi.repository import Gio, GLib
    inhibitors = Inhibitors()
    inhibitors.hold.recover()
    watches = {}
    loop = GLib.MainLoop()
    state = {"idle_since": GLib.get_monotonic_time()}

    def watch(connection, sender):
        if sender in watches:
            return
        def vanished(_connection, name):
            Gio.bus_unwatch_name(watches.pop(name))
            if name in inhibitors.senders(): log(f"Programme parti sans libérer ({name}) : ses demandes tombent")
            inhibitors.forget(name)
        watches[sender] = Gio.bus_watch_name_on_connection(connection, sender, Gio.BusNameWatcherFlags.NONE,
                                                            None, vanished)

    def call(connection, sender, _path, _interface, method, parameters, invocation):
        if method == "Inhibit":
            application, reason = parameters.unpack()
            cookie = inhibitors.add(sender, application, reason)
            watch(connection, sender)
            log(f"Économiseur bloqué ({cookie}) : {application} — {reason}")
            invocation.return_value(GLib.Variant("(u)", (cookie,)))
        elif method == "UnInhibit":
            log(f"Demande libérée ({parameters.unpack()[0]})")
            inhibitors.remove(parameters.unpack()[0])
            invocation.return_value(None)
        elif method == "SimulateUserActivity":
            wake_screen()
            invocation.return_value(None)
        elif method == "GetActive":
            invocation.return_value(GLib.Variant("(b)", (False,)))
        elif method == "GetActiveTime":
            invocation.return_value(GLib.Variant("(u)", (0,)))
        elif method == "SetActive":
            invocation.return_value(GLib.Variant("(b)", (False,)))

    def tick():
        if inhibitors.items:
            wake_screen()
            inhibitors.hold.reassert()
            state["idle_since"] = GLib.get_monotonic_time()
        elif GLib.get_monotonic_time() - state["idle_since"] > IDLE_EXIT_SECONDS * 1_000_000:
            loop.quit()   # relancé par D-Bus à la prochaine demande
        return True

    node = Gio.DBusNodeInfo.new_for_xml(XML)
    methods = {method.name: "".join(arg.signature for arg in method.in_args) for method in node.interfaces[0].methods}
    def acquired(connection, _name):
        # Pas de register_object (#3341) : PyGObject y garde chaque invocation.
        connection.add_filter(bus_filter(methods, call))
    Gio.bus_own_name(Gio.BusType.SESSION, NAME,
                     Gio.BusNameOwnerFlags.ALLOW_REPLACEMENT | Gio.BusNameOwnerFlags.REPLACE,
                     acquired, None, lambda *_: loop.quit())
    GLib.timeout_add_seconds(POKE_SECONDS, tick)
    # Arrêt par signal (bus de session fermé, kill) : on sort par la fin de
    # boucle, qui rend le DPMS, au lieu de le laisser coupé (#735).
    for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        GLib.unix_signal_add(GLib.PRIORITY_HIGH, signum, lambda *_: loop.quit() or False)
    loop.run()
    inhibitors.items.clear()
    inhibitors.sync()
    return 0


if __name__ == "__main__":
    sys.exit(main())
