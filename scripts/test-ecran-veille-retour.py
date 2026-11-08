#!/usr/bin/env python3
"""#735 : la veille de l'écran revient quand la dernière demande « écran allumé » tombe.

Après une vidéo (org.freedesktop.ScreenSaver.Inhibit), le DPMS du serveur X
restait coupé jusqu'à la fin de la session :
- le retour ne dépendait que d'une marque sur disque, qu'une autre instance
  effaçait même si son propriétaire vivait ;
- le service arrêté par un signal ne rendait rien ;
- XScreenSaver (dpmsEnabled: False par défaut) coupait lui-même le DPMS au
  premier affichage de l'économiseur.

Le serveur X est remplacé par un faux `xset` qui garde l'état du DPMS.
"""
import ast
import os
from pathlib import Path
import runpy
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
tool = runpy.run_path(str(ROOT / "scripts/winlux-screensaver-inhibit.py"), run_name="veille_test")
DisplayHold, Inhibitors, SaverDpms = tool["DisplayHold"], tool["Inhibitors"], tool["SaverDpms"]


class FakeX:
    """DPMS du serveur X : activé au départ, comme sur une session neuve."""

    def __init__(self):
        self.enabled = True
        self.calls = []

    def __call__(self, *args):
        self.calls.append(args)
        if args == ("q",): return f"  DPMS is {'Enabled' if self.enabled else 'Disabled'}\n"
        if args == ("-dpms",): self.enabled = False
        if args == ("+dpms",): self.enabled = True
        return ""


with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)

    # 1. Marque perdue pendant la demande : la veille revient quand même.
    x = FakeX()
    hold = DisplayHold(xset=x, marker=tmp / "m1")
    hold.start(); hold.reassert()
    assert not x.enabled
    (tmp / "m1").unlink()
    hold.reassert()
    assert not x.enabled, "marque perdue : le DPMS doit rester suspendu tant que la demande tient"
    hold.stop()
    assert x.enabled, f"marque perdue : DPMS jamais rétabli ({x.calls})"

    # 2. Une seconde instance ne vole pas la marque d'un service vivant.
    x = FakeX()
    first = DisplayHold(xset=x, marker=tmp / "m2")
    first.start()
    other = DisplayHold(xset=x, marker=tmp / "m2")
    other.pid = first.pid + 100000   # autre processus ; le premier (nous) est vivant
    other.recover()
    assert not x.enabled and (tmp / "m2").exists(), "recover() a rendu le DPMS d'une demande encore tenue"
    first.stop()
    assert x.enabled and not (tmp / "m2").exists()
    # … mais rattrape celle d'un service mort.
    x = FakeX()
    (tmp / "m3").write_text("999999999\n"); x.enabled = False
    DisplayHold(xset=x, marker=tmp / "m3").recover()
    assert x.enabled and not (tmp / "m3").exists(), "marque d'un service mort non rattrapée"
    # Ancienne marque (« 1 », sans numéro) : rattrapée aussi.
    x = FakeX(); (tmp / "m4").write_text("1\n"); x.enabled = False
    DisplayHold(xset=x, marker=tmp / "m4").recover()
    assert x.enabled

    # 3. DPMS coupé par l'utilisateur avant la demande : laissé tel quel.
    x = FakeX(); x.enabled = False
    hold = DisplayHold(xset=x, marker=tmp / "m5")
    hold.start(); hold.stop()
    assert not x.enabled and ("+dpms",) not in x.calls

    # 4. Plusieurs demandes simultanées, programmes qui quittent sans UnInhibit.
    x = FakeX()
    box = Inhibitors(poke=lambda: None, inhibit_file=tmp / "inhibit" / "1-screensaver", hold=DisplayHold(xset=x, marker=tmp / "m6"))
    a = box.add(":1.5", "firefox", "video-playing")
    box.add(":1.5", "firefox", "audio-playing")
    c = box.add(":1.9", "mpv", "video")
    assert not x.enabled
    box.forget(":1.5")             # Firefox tué, sans libérer
    assert not x.enabled, "une demande de mpv tient encore"
    box.remove(a)                  # cookie déjà tombé : sans effet
    assert not x.enabled
    box.remove(c)
    assert x.enabled and not box.items and not (tmp / "inhibit" / "1-screensaver").exists(), x.calls
print("DisplayHold : marque perdue, seconde instance, DPMS de l'utilisateur, demandes multiples et client mort")

# 4b. #749 : xscreensaver-auth --init coupe toujours le DPMS au démarrage
# d'XScreenSaver ; Winlux le remet une fois --init terminé.
x, state, clock = FakeX(), {"daemons": set(), "init": False, "held": False, "wanted": True}, [0.0]
saver = SaverDpms(xset=x, processes=lambda: (state["daemons"], state["init"]), wanted=lambda: state["wanted"],
                  held=lambda: state["held"], clock=lambda: clock[0])
saver.tick(); assert x.enabled and x.calls == [], "sans XScreenSaver : rien"
state["daemons"], state["init"] = {4242}, True
x.enabled = False; saver.tick()
assert not x.enabled, "--init encore en cours : on attend"
state["init"] = False; clock[0] = 2; saver.tick()
assert x.enabled, f"--init terminé : DPMS non rétabli ({x.calls})"
x.enabled = False; clock[0] = 10; state["held"] = True; saver.tick()
assert not x.enabled, "une vidéo tient l'écran : le DPMS reste suspendu"
state["held"] = False; state["wanted"] = False; saver.tick()
assert not x.enabled, "dpmsEnabled: False choisi : respecté"
state["wanted"] = True; clock[0] = 100; saver.tick()
assert not x.enabled, "hors de la minute qui suit le démarrage : plus d'intervention"
state["daemons"] = {5151}; saver.tick()
assert x.enabled, "XScreenSaver relancé : DPMS remis à nouveau"
with tempfile.TemporaryDirectory() as tmp:
    proc = Path(tmp)
    for pid, argv in ((10, b"/usr/bin/xscreensaver\0-no-splash\0"), (11, b"/usr/libexec/xscreensaver/xscreensaver-auth\0--init\0"),
                      (12, b"/usr/bin/xscreensaver-command\0-deactivate\0"), (13, b"python3\0xscreensaver\0")):
        (proc / str(pid)).mkdir(); (proc / str(pid) / "cmdline").write_bytes(argv)
    (proc / "self").mkdir()
    assert tool["saver_processes"](str(proc)) == ({10}, True), tool["saver_processes"](str(proc))
    (proc / "11/cmdline").write_bytes(b"/usr/libexec/xscreensaver/xscreensaver-auth\0--splash\0")
    assert tool["saver_processes"](str(proc)) == ({10}, False)
    rc = Path(tmp) / "rc"
    assert tool["dpms_wanted"](rc), "sans fichier : actif"
    rc.write_text("timeout: 0:10:00\ndpmsEnabled:    False\n"); assert not tool["dpms_wanted"](rc)
    rc.write_text("dpmsEnabled: True\n"); assert tool["dpms_wanted"](rc)
source_tool = (ROOT / "scripts/winlux-screensaver-inhibit.py").read_text(encoding="utf-8")
assert "saver = SaverDpms()" in source_tool.split("def watch_updates")[1], "SaverDpms non lancé avec la session"
print("#749 : DPMS remis après xscreensaver-auth --init (vidéo en cours et choix de l'utilisateur respectés)")

# 4c. #750 : sortie vers /dev/null (dbus-daemon --fork) : journal propre au service.
with tempfile.TemporaryDirectory() as tmp:
    code = f"""import runpy, sys
tool = runpy.run_path({str(ROOT / "scripts/winlux-screensaver-inhibit.py")!r}, run_name="x")
print(tool["own_log"](directory={tmp!r}), file=open({tmp + "/path"!r}, "w"))
tool["log"]("Économiseur bloqué (1) : firefox — video-playing")
"""
    subprocess.run([sys.executable, "-B", "-c", code], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
    text = (Path(tmp) / "screensaver-inhibit.log").read_text()
    assert "Économiseur bloqué (1)" in text, text
    out = subprocess.run([sys.executable, "-B", "-c", code], capture_output=True, text=True, check=True)
    assert "Économiseur bloqué (1)" in out.stdout and (Path(tmp) / "path").read_text().strip() == "None", "sortie visible : pas de redirection"
print("#750 : service activé par D-Bus : screensaver-inhibit.log dans les journaux de session")

# 5. XScreenSaver ne coupe plus le DPMS (dpmsEnabled: True), comptes déjà copiés compris.
template = ROOT / "integration/native/xscreensaver"
keys = dict(line.split(":", 1) for line in template.read_text().splitlines() if ":" in line)
assert keys.get("dpmsEnabled", "").strip() == "True", keys
assert all(keys.get(k, "").strip() == "0:10:00" for k in ("dpmsStandby", "dpmsSuspend", "dpmsOff")), keys
source = (ROOT / "scripts/winlux-native-config.py").read_text(encoding="utf-8")
assert 'migrate_xscreensaver(home / ".xscreensaver", source / "xscreensaver")' in source
function = next(node for node in ast.parse(source).body if isinstance(node, ast.FunctionDef) and node.name == "migrate_xscreensaver")
space = {"Path": Path}
exec(compile(ast.Module(body=[function], type_ignores=[]), "winlux-native-config.py", "exec"), space)
with tempfile.TemporaryDirectory() as tmp:
    old = Path(tmp) / ".xscreensaver"
    old.write_text("timeout:        0:10:00\nmode:           one\n")
    space["migrate_xscreensaver"](old, template)
    text = old.read_text()
    assert "dpmsEnabled:    True" in text and "timeout:        0:10:00" in text, text
    space["migrate_xscreensaver"](old, template)
    assert old.read_text() == text, "migration répétée"
    chosen = Path(tmp) / "choisi"
    chosen.write_text("timeout: 0:05:00\ndpmsEnabled: False\n")
    space["migrate_xscreensaver"](chosen, template)
    assert chosen.read_text() == "timeout: 0:05:00\ndpmsEnabled: False\n", "choix de l'utilisateur écrasé"
print("XScreenSaver : dpmsEnabled True dans le modèle et ajouté aux comptes existants, choix explicite gardé")

# 6. Service réel sur un bus de session.
try:
    from gi.repository import Gio  # noqa: F401
    usable = bool(shutil.which("dbus-daemon") and shutil.which("dbus-run-session"))
except Exception:
    usable = False
if not usable:
    print("(pas de D-Bus ou de PyGObject ici : essai du service réel sauté)")
    sys.exit(0)

with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    (tmp / "bin").mkdir()
    state = tmp / "dpms-off"
    (tmp / "bin/xset").write_text(f'''#!/bin/sh
case "$*" in
  q) if test -e "{state}"; then echo "  DPMS is Disabled"; else echo "  DPMS is Enabled"; fi ;;
  -dpms) : >"{state}" ;;
  +dpms) rm -f "{state}" ;;
esac
exit 0
''')
    (tmp / "bin/xscreensaver-command").write_text("#!/bin/sh\nexit 0\n")
    for name in ("xset", "xscreensaver-command"): (tmp / "bin" / name).chmod(0o755)
    client = tmp / "client.py"
    client.write_text('''import sys, time
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
def call(method, args, reply=None):
    return bus.call_sync("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", method, args, reply, 0, 5000, None)
cookie = call("Inhibit", GLib.Variant("(ss)", (sys.argv[1], "video-playing")), GLib.VariantType("(u)")).unpack()[0]
print("cookie", cookie, flush=True)
for line in sys.stdin:
    if line.strip() == "release": call("UnInhibit", GLib.Variant("(u)", (cookie,))); print("released", flush=True)
time.sleep(30)
''')
    script = tmp / "scenario.py"
    script.write_text(f'''import os, signal, subprocess, sys, time
from pathlib import Path
state = Path({str(state)!r})
def dpms(): return "Disabled" if state.exists() else "Enabled"
def wait_for(wanted, what):
    for _ in range(50):
        if dpms() == wanted: return
        time.sleep(0.1)
    raise SystemExit(f"{{what}} : DPMS {{dpms()}} au lieu de {{wanted}}")
service = subprocess.Popen([sys.executable, "-B", {str(ROOT / "scripts/winlux-screensaver-inhibit.py")!r}],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)   # comme sous dbus-daemon --fork
time.sleep(1)
def client(name):
    p = subprocess.Popen([sys.executable, "-B", {str(client)!r}, name], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    assert p.stdout.readline().startswith("cookie"), name
    return p
firefox = client("firefox"); mpv = client("mpv")
wait_for("Disabled", "deux demandes")
firefox.kill(); firefox.wait()      # Firefox meurt sans libérer
time.sleep(1)
wait_for("Disabled", "mpv tient encore")
mpv.stdin.write("release\\n"); mpv.stdin.flush()
assert mpv.stdout.readline().strip() == "released"
wait_for("Enabled", "dernière demande libérée")
mpv.kill(); mpv.wait()
# Service arrêté par un signal pendant une demande : la veille revient.
video = client("firefox")
wait_for("Disabled", "nouvelle demande")
service.send_signal(signal.SIGTERM); service.wait(timeout=10)
wait_for("Enabled", "service arrêté par SIGTERM")
video.kill(); video.wait()
print("scénario réel OK")
''')
    env = dict(os.environ, PATH=f"{tmp / 'bin'}:{os.environ['PATH']}", WINLUX_POWER_INHIBIT_DIR=str(tmp / "inhibit"),
               XDG_RUNTIME_DIR=str(tmp), DISPLAY=":735", WINLUX_SESSION_LOGS=str(tmp / "logs"))
    out = subprocess.run(["dbus-run-session", "--", sys.executable, "-B", str(script)], env=env, capture_output=True, text=True, timeout=90)
    assert out.returncode == 0 and "scénario réel OK" in out.stdout, out.stdout + out.stderr
    journal = (tmp / "logs/screensaver-inhibit.log").read_text()
    for line in ("Économiseur bloqué (1) : firefox", "Programme parti sans libérer", "Demande libérée", "Veille de l'écran rétablie"):
        assert line in journal, journal
print("Service réel : deux demandes, client tué sans libérer, libération de la dernière, arrêt par SIGTERM : la veille revient")
