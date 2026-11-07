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
s'il était actif ; une marque dans $XDG_RUNTIME_DIR le rétablit aussi après un
arrêt brutal du service.
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys

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
POKE_SECONDS = 50
INHIBIT_DIR = Path(os.environ.get("WINLUX_POWER_INHIBIT_DIR", "/run/winlux/power-inhibit"))
IDLE_EXIT_SECONDS = 600


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
            print(f"Demande d'éveil non posée ({self.inhibit_file}) : {error}", file=sys.stderr, flush=True)


def _xset(*args):
    if not shutil.which("xset"): return ""
    try:
        return subprocess.run(["xset", *args], capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


class DisplayHold:
    """DPMS du serveur X suspendu tant qu'une demande est posée."""

    def __init__(self, xset=_xset, marker=None):
        self.xset = xset
        runtime = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
        display = "".join(c if c.isalnum() else "_" for c in os.environ.get("DISPLAY", "none"))
        self.marker = Path(marker) if marker else Path(runtime) / f"winlux-dpms-suspendu-{display}"
        self.active = False

    def recover(self):
        """Service arrêté brutalement pendant une demande : DPMS rétabli."""
        if self.marker.exists() and not self.active:
            self.xset("+dpms")
            self.marker.unlink(missing_ok=True)

    def start(self):
        if not self.active:
            self.active = True
            if "DPMS is Enabled" in self.xset("q"):
                try: self.marker.write_text("1\n")
                except OSError: pass
        self.reassert()

    def reassert(self):
        if self.active and self.marker.exists():
            self.xset("-dpms")

    def stop(self):
        if not self.active: return
        self.active = False
        if self.marker.exists():
            self.xset("+dpms")
            self.marker.unlink(missing_ok=True)


def wake_screen():
    for command in (["xscreensaver-command", "-deactivate"], ["xset", "s", "reset"]):
        if shutil.which(command[0]):
            try:
                subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
            except (OSError, subprocess.SubprocessError):
                pass


def main():
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
            inhibitors.forget(name)
        watches[sender] = Gio.bus_watch_name_on_connection(connection, sender, Gio.BusNameWatcherFlags.NONE,
                                                            None, vanished)

    def call(connection, sender, _path, _interface, method, parameters, invocation):
        if method == "Inhibit":
            application, reason = parameters.unpack()
            cookie = inhibitors.add(sender, application, reason)
            watch(connection, sender)
            print(f"Économiseur bloqué ({cookie}) : {application} — {reason}", flush=True)
            invocation.return_value(GLib.Variant("(u)", (cookie,)))
        elif method == "UnInhibit":
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
    def acquired(connection, _name):
        for path in PATHS:
            connection.register_object(path, node.interfaces[0], call, None, None)
    Gio.bus_own_name(Gio.BusType.SESSION, NAME,
                     Gio.BusNameOwnerFlags.ALLOW_REPLACEMENT | Gio.BusNameOwnerFlags.REPLACE,
                     acquired, None, lambda *_: loop.quit())
    GLib.timeout_add_seconds(POKE_SECONDS, tick)
    loop.run()
    inhibitors.items.clear()
    inhibitors.sync()
    return 0


if __name__ == "__main__":
    sys.exit(main())
