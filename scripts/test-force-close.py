#!/usr/bin/python3
"""« <Nom> ne répond pas » : croix, Alt+F4 et « Forcer la fermeture ».

À lancer dans un affichage X dédié (xvfb-run). Démarre Openbox avec le rc.xml
livré, puis des fenêtres d'essai GTK : une qui ignore WM_DELETE_WINDOW tout en
répondant au ping (programme bloqué vu de l'utilisateur), une qui se ferme
normalement, une qui pose une question avant de se fermer. Vérifie :
  - la boîte apparaît après le délai et « Fermer le programme » termine le processus ;
  - « Attendre » le laisse vivre ;
  - une fenêtre qui se ferme, ou qui ouvre une question, ne déclenche rien ;
  - un PID d'un autre utilisateur, init ou une autre machine ne sont jamais tués ;
  - menu de fenêtre traduit, migration d'un rc.xml existant.
Sans Openbox ni xprop, seules les vérifications hors affichage tournent.

    test-force-close.py
    test-force-close.py window hung|normal|dialog    (interne : fenêtre d'essai)
"""
import ast
import os
from pathlib import Path
import pwd
import re
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
NS = "{http://openbox.org/3.4/rc}"
problems = []


def check(condition, message):
    if not condition:
        problems.append(message)


def test_window(mode):
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk

    window = Gtk.Window(title=f"Essai {mode} - Programme Essai")
    window.set_default_size(320, 200)

    def on_delete(*_):
        if mode == "normal":
            Gtk.main_quit()
            return False
        if mode == "dialog":
            question = Gtk.Window(title="Enregistrer les modifications ?")
            question.set_transient_for(window)
            question.show_all()
        return True  # « bloqué » : la demande de fermeture n'est jamais traitée
    window.connect("delete-event", on_delete)
    window.show_all()
    Gtk.main()


def xprop(*args):
    return subprocess.run(["xprop", *args], capture_output=True, text=True, timeout=5).stdout


def find_window(pid, deadline=15):
    end = time.monotonic() + deadline
    while time.monotonic() < end:
        listing = xprop("-root", "_NET_CLIENT_LIST").split("=", 1)[-1]
        for window in re.findall(r"0x[0-9a-fA-F]+", listing):
            if re.search(rf"= {pid}\b", xprop("-id", window, "_NET_WM_PID")):
                return window
        time.sleep(0.2)
    return None


def gone(process, seconds):
    try:
        process.wait(timeout=seconds)
        return True
    except subprocess.TimeoutExpired:
        return False


def offline_checks():
    # Menu de fenêtre : XML d'Openbox valide, « Forcer la fermeture » sur $wid, quatre langues.
    for lang, label in (("fr", "Forcer la fermeture"), ("en", "Force close"), ("es", "Forzar cierre"), ("de", "Schließen erzwingen")):
        output = subprocess.run([sys.executable, str(HERE / "winlux-force-close.py"), "menu"], capture_output=True, text=True,
                                env=dict(os.environ, WINLUX_LANG=lang)).stdout
        try:
            root = ET.fromstring(output)
        except ET.ParseError as error:
            problems.append(f"menu de fenêtre ({lang}) illisible : {error}")
            continue
        labels = {item.get("label"): ET.tostring(item, encoding="unicode") for item in root.iter("item")}
        check(label in labels and "winlux-force-close $wid" in labels[label], f"menu de fenêtre ({lang}) : « {label} » absent")
    import runpy
    module = runpy.run_path(str(HERE / "winlux-force-close.py"), run_name="essai")
    check(module["_TR"] and all(len(row) == 3 and all(row) for row in module["_TR"].values()), "traductions incomplètes")
    refusal = module["refusal"]
    check(refusal({"pid": 1, "machine": None}) is not None, "init (PID 1) accepté")
    check(refusal({"pid": 0, "machine": None}) is not None, "PID 0 accepté")
    check(refusal({"pid": os.getpid(), "machine": None}) is not None, "le script lui-même accepté")
    child = subprocess.Popen(["sleep", "30"])
    try:
        check(refusal({"pid": child.pid, "machine": None}) is None, "processus du compte refusé")
        check(refusal({"pid": child.pid, "machine": "autre-pc"}) is not None, "fenêtre d'une autre machine acceptée")
    finally:
        child.kill()
    # rc.xml livré : croix, Alt+F4, clic droit sur la barre de titre, Alt+Espace.
    shipped = HERE.parent / "integration/native/config/openbox/rc.xml"
    root = ET.parse(shipped).getroot()
    close = next(item for item in root.find(f"{NS}mouse") if item.get("name") == "Close")
    commands = [item.text for item in close.iter(f"{NS}command")]
    names = [item.get("name") for item in close.iter(f"{NS}action")]
    check(names == ["Close", "Execute"] and commands == ["winlux-close-watch $wid"], f"croix : {names} {commands}")
    keys = {item.get("key"): item for item in root.find(f"{NS}keyboard")}
    check("winlux-close-watch $wid" in ET.tostring(keys["A-F4"], encoding="unicode"), "Alt+F4 sans winlux-close-watch")
    check("A-space" not in keys, "Alt+Espace pris par openbox : les applications ne le reçoivent pas (#1168)")
    menus = [item.text for item in root.iter(f"{NS}menu") if item.text]
    check("winlux-window-menu" in menus and "client-menu" not in menus, f"menu de la barre de titre : {menus}")
    files = [item.text for item in root.find(f"{NS}menu").findall(f"{NS}file")]
    check("/usr/share/winlux/native/config/openbox/window-menu.xml" in files, "fichier du menu de fenêtre non chargé")
    menu_file = HERE.parent / "integration/native/config/openbox/window-menu.xml"
    check('execute="/usr/bin/winlux-force-close menu"' in menu_file.read_text(encoding="utf-8"), "menu de fenêtre livré")
    stage = (HERE / "stage-desktop-components.sh").read_text(encoding="utf-8")
    check(" winlux-force-close " in stage and " winlux-close-watch " in stage, "scripts non installés dans l'image")
    # Compte existant : rc.xml d'avant mis à jour à l'ouverture de session.
    namespace = {"ET": ET, "sys": sys, "_t": lambda text: text}
    source = (HERE / "winlux-native-config.py").read_text(encoding="utf-8")
    for node in ast.parse(source).body:
        if (isinstance(node, ast.Assign) and any(getattr(t, "id", "") in ("ADDED_KEYS", "REPLACED_KEYS", "OLD_SNAP_ACTIONS") for t in node.targets)) \
                or (isinstance(node, ast.FunctionDef) and node.name == "migrate_openbox"):
            exec(compile(ast.Module([node], []), "winlux-native-config", "exec"), namespace)
    old = """<?xml version="1.0"?><openbox_config xmlns="http://openbox.org/3.4/rc"><keyboard>
<keybind key="A-F4"><action name="Close"/></keybind>
<keybind key="A-space"><action name="ShowMenu"><menu>client-menu</menu></action></keybind></keyboard><mouse>
<context name="Titlebar"><mousebind button="Right" action="Press"><action name="ShowMenu"><menu>client-menu</menu></action></mousebind></context>
<context name="Close"><mousebind button="Left" action="Click"><action name="Close"/></mousebind></context></mouse>
<menu><file>~/.config/openbox/menu.xml</file><hideDelay>200</hideDelay></menu></openbox_config>"""
    with tempfile.TemporaryDirectory() as folder:
        user = Path(folder) / "rc.xml"
        user.write_text(old)
        namespace["migrate_openbox"](user, shipped)
        text = user.read_text(encoding="utf-8")
        check(text.count("winlux-close-watch $wid") == 2, "migration : croix et Alt+F4")
        check("client-menu" not in text and text.count("winlux-window-menu") == 1, "migration : menu de fenêtre (barre de titre)")
        check("window-menu.xml" in text, "migration : fichier de menu")
        check('key="A-space"' not in text, "migration : Alt+Espace rendu aux applications (#1168)")
        before = user.read_bytes()
        namespace["migrate_openbox"](user, shipped)
        check(user.read_bytes() == before, "migration : deuxième passage sans changement")
        # Alt+Espace personnel (une commande) : gardé.
        user.write_text(old.replace('<action name="ShowMenu"><menu>client-menu</menu></action></keybind></keyboard>',
                                    '<action name="Execute"><command>mon-lanceur</command></action></keybind></keyboard>'))
        namespace["migrate_openbox"](user, shipped)
        check('key="A-space"' in user.read_text(encoding="utf-8"), "migration : Alt+Espace personnel gardé")


def other_user_pid():
    """Un processus d'un autre compte : créé sous « nobody » si l'on est root, sinon trouvé."""
    if os.getuid() == 0:
        nobody = pwd.getpwnam("nobody").pw_uid
        process = subprocess.Popen(["sleep", "60"], user=nobody, group=nobody)
        return process.pid, process
    for entry in Path("/proc").iterdir():
        if entry.name.isdigit() and int(entry.name) > 1:
            try:
                if entry.stat().st_uid != os.getuid():
                    return int(entry.name), None
            except OSError:
                pass
    return None, None


def display_checks(temp):
    rc = temp / "rc.xml"
    text = (HERE.parent / "integration/native/config/openbox/rc.xml").read_text(encoding="utf-8")
    text = text.replace("winlux-close-watch $wid", f"{sys.executable} -B {HERE / 'winlux-close-watch.py'} $wid")
    rc.write_text(text, encoding="utf-8")
    log = temp / "openbox.log"
    env = dict(os.environ, WINLUX_CLOSE_WATCH_DELAY="1.5", WINLUX_FORCE_CLOSE_GRACE="1", XDG_RUNTIME_DIR=str(temp))

    def start_openbox(answer):
        return subprocess.Popen(["openbox", "--config-file", str(rc)], env=dict(env, WINLUX_FORCE_CLOSE_ANSWER=answer),
                                stdout=open(log, "a"), stderr=subprocess.STDOUT)

    def launch(mode):
        process = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()), "window", mode], env=env,
                                   stderr=subprocess.DEVNULL)
        window = find_window(process.pid)
        check(window is not None, f"fenêtre d'essai {mode} introuvable")
        if window:
            subprocess.run(["xdotool", "windowactivate", "--sync", window], capture_output=True, timeout=5)
            time.sleep(0.3)
        return process, window

    def close(window):
        # Alt+F4 passe par la liaison livrée : Close puis winlux-close-watch $wid.
        subprocess.run(["xdotool", "key", "alt+F4"], capture_output=True, timeout=5)

    for answer in ("close", "wait"):
        openbox = start_openbox(answer)
        time.sleep(1)
        try:
            # 1. Programme bloqué : la boîte apparaît après le délai.
            process, window = launch("hung")
            if window:
                close(window)
                if answer == "close":
                    check(gone(process, 10), "programme bloqué : « Fermer le programme » ne l'a pas terminé")
                    time.sleep(1)  # la trace de winlux-force-close suit la fin du processus
                    check("fermé" in log.read_text(errors="replace"), "programme bloqué : pas de trace de la fermeture forcée")
                else:
                    check(not gone(process, 5), "« Attendre » : le programme a été tué")
            process.kill()
            if answer == "wait":
                continue
            # 2. Fermeture normale : rien ne se déclenche.
            before = log.read_text(errors="replace").count("fermé")
            process, window = launch("normal")
            if window:
                close(window)
                check(gone(process, 5), "fenêtre normale : ne s'est pas fermée")
                time.sleep(3)
                check(log.read_text(errors="replace").count("fermé") == before, "fenêtre normale : fermeture forcée déclenchée")
            # 3. Question « Enregistrer ? » : le programme répond, rien ne se déclenche.
            process, window = launch("dialog")
            if window:
                close(window)
                check(not gone(process, 5), "question avant fermeture : programme tué alors qu'il répond")
            process.kill()
            # 4. Fenêtre attribuée à un autre compte, à init, à une autre machine : jamais tué.
            process, window = launch("hung")
            if window:
                other, owned = other_user_pid()
                for pid, machine, what in ((other, None, "autre utilisateur"), (1, None, "init"),
                                           (process.pid, "autre-pc", "autre machine")):
                    if pid is None:
                        continue
                    subprocess.run(["xprop", "-id", window, "-f", "_NET_WM_PID", "32c", "-set", "_NET_WM_PID", str(pid)], check=True)
                    if machine:
                        subprocess.run(["xprop", "-id", window, "-f", "WM_CLIENT_MACHINE", "8s", "-set", "WM_CLIENT_MACHINE", machine], check=True)
                    result = subprocess.run([sys.executable, "-B", str(HERE / "winlux-force-close.py"), window], capture_output=True,
                                            text=True, env=dict(env, WINLUX_FORCE_CLOSE_ANSWER="close"))
                    check(result.returncode == 1 and "refusé" in result.stderr, f"{what} : non refusé ({result.stderr.strip()})")
                    check(Path(f"/proc/{pid}").exists(), f"{what} : processus tué")
                check(process.poll() is None, "programme d'essai tué malgré les refus")
                if owned:
                    owned.kill()
            process.kill()
        finally:
            openbox.terminate()
            openbox.wait(timeout=10)


def main():
    offline_checks()
    missing = [tool for tool in ("openbox", "xprop", "xdotool") if not shutil.which(tool)]
    if missing or not os.environ.get("DISPLAY"):
        print("Fenêtres réelles non vérifiées : " + (", ".join(missing) or "DISPLAY") + " absent(s)")
    else:
        with tempfile.TemporaryDirectory() as temp:
            display_checks(Path(temp))
    for problem in problems:
        print("ÉCHEC : " + problem, file=sys.stderr)
    if not problems:
        print("OK: « ne répond pas » après la croix, fermeture forcée limitée au compte, menu de fenêtre, migration")
    return 1 if problems else 0


if __name__ == "__main__":
    if sys.argv[1:2] == ["window"]:
        test_window(sys.argv[2])
    else:
        sys.exit(main())
