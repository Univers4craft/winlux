#!/usr/bin/python3
"""Run in xvfb-run dbus-run-session: no host preferences or hardware writes."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import time
from gi.repository import Gio, GLib

project=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix="winlux-desktop-check.") as temporary:
    task_directory=Path(temporary)
    environment=dict(os.environ,XDG_STATE_HOME=str(task_directory/"state"),XDG_CONFIG_HOME=str(task_directory/"config"))
    preference=task_directory/"config/winlux/do-not-disturb"
    preference.parent.mkdir(parents=True);preference.touch()
    process=subprocess.Popen(["/usr/bin/python3","-B",str(project/"winlux-notifications.py")],env=environment)
    try:
        bus=Gio.bus_get_sync(Gio.BusType.SESSION,None)
        def call(interface,method,params=None):
            return bus.call_sync("org.freedesktop.Notifications","/org/freedesktop/Notifications",interface,method,
                params,None,Gio.DBusCallFlags.NONE,3000,None)
        for attempt in range(40):
            try:
                call("org.freedesktop.Notifications","GetCapabilities");break
            except GLib.Error:
                if process.poll() is not None:raise RuntimeError("Notification service exited")
                time.sleep(.05)
        args=GLib.Variant("(susssasa{sv}i)",("Winlux test",0,"","Notification test","Body",["open","Ouvrir"],{},1000))
        identity=call("org.freedesktop.Notifications","Notify",args).unpack()[0]
        history=json.loads((task_directory/"state/winlux/notifications.json").read_text())
        assert history[-1]["id"]==identity and history[-1]["title"]=="Notification test"
        # Croix du panneau Notifications : une seule notification retirée de l'historique.
        other=call("org.freedesktop.Notifications","Notify",GLib.Variant("(susssasa{sv}i)",("Winlux test",0,"","Autre","Corps",[],{},1000))).unpack()[0]
        call("org.winlux.Notifications","Dismiss",GLib.Variant("(u)",(other,)))
        assert [item["id"] for item in json.loads((task_directory/"state/winlux/notifications.json").read_text())]==[identity]
        call("org.winlux.Notifications","Activate",GLib.Variant("(us)",(identity,"open")))
        call("org.winlux.Notifications","Clear")
        assert json.loads((task_directory/"state/winlux/notifications.json").read_text())==[]
        print("OK: notification D-Bus, historique, action et effacement avec Ne pas déranger")
    finally:
        process.terminate();process.wait(timeout=5)

module=runpy.run_path(str(project/"winlux-system-tools.py"),run_name="tools_test")
Gtk,GLib=module["Gtk"],module["GLib"]
assert module["split_nmcli"](r"aa\:bb\:cc\:dd\:ee\:ff:Nom\: invité:90:WPA2") == ["aa:bb:cc:dd:ee:ff", "Nom: invité", "90", "WPA2"]
windows=[module["Tools"](page) for page in ("quick","notifications","tasks","appearance","apps","files","recovery")]
assert all(window.process is None for window in windows)
def close():
    for window in windows:window.destroy()
    return False
GLib.timeout_add(200,close);Gtk.main()
print("OK: sept outils construits sans action système et décodage des réseaux")
