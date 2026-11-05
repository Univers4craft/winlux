#!/usr/bin/python3
"""Raccourcis Windows (#474) : Win+I, Win+S, Win+R (Exécuter), Win+X (accès rapide), Win+P.

    python3 -B scripts/test-run-dialog.py          (boîte réelle si DISPLAY et xdotool)
"""
import ast
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
run = runpy.run_path(str(HERE / "winlux-run.py"), run_name="run_test")
resolve = run["resolve"]

# ── Saisies de la boîte Exécuter ──────────────────────────────────────────
with tempfile.TemporaryDirectory() as folder:
    home = Path(folder)
    (home / "Documents").mkdir()
    (home / "Documents/lettre.txt").write_text("x")
    (home / "Bureau").mkdir()
    (home / "Bureau/putty.exe").write_bytes(b"MZ")
    which = lambda name: "/usr/bin/" + name if name in ("ls", "gimp") else None
    r = lambda text: resolve(text, home, which)
    assert r("cmd") == ["winlux-terminal"] and r("CMD.EXE") == ["winlux-terminal"] and r("powershell") == ["winlux-terminal"]
    assert r("notepad") == ["winlux-shell-app", "editor"] and r("notepad.exe ~/a.txt") == ["winlux-shell-app", "editor", "~/a.txt"]
    assert r("control") == ["winlux-shell-app", "settings"] and r("ms-settings:display") == ["winlux-shell-app", "settings"]
    assert r("appwiz.cpl") == ["winlux-shell-app", "settings", "1"] and r("devmgmt.msc") == ["winlux-shell-app", "settings", "34"]
    assert r("taskmgr") == ["winlux-shell-app", "taskmgr"] and r("regedit") == ["winlux", "--regedit"]
    assert r("msinfo32") == ["winlux-shell-app", "msinfo32"] and r("eventvwr.msc") == ["winlux-shell-app", "eventviewer"]
    assert r("explorer") == ["winlux-shell-app", "explorer"]
    assert r("explorer C:\\Users\\damien\\Documents") == ["winlux-shell-app", "explorer", str(home / "Documents")]
    assert r("https://univers4craft.github.io/winlux/") == ["xdg-open", "https://univers4craft.github.io/winlux/"]
    assert r("www.example.org") == ["xdg-open", "https://www.example.org"]
    assert r(str(home / "Documents")) == ["winlux-shell-app", "explorer", str(home / "Documents")]
    assert r("C:\\Users\\damien\\Documents\\lettre.txt") == ["xdg-open", str(home / "Documents/lettre.txt")]
    assert r(str(home / "Bureau/putty.exe")) == ["winlux-launch-exe", "desktop", str(home / "Bureau/putty.exe")]
    assert r("Z:\\etc") == ["winlux-shell-app", "explorer", "/etc"]
    assert r("ls -l") == ["ls", "-l"] and r("gimp.exe") == ["gimp"]
    assert r("introuvable-xyz") is None and r("   ") is None and r(str(home / "absent")) is None
for name, command in run["ALIASES"].items():
    assert command and command[0] in ("winlux-terminal", "winlux-shell-app", "winlux"), name
for key, row in run["_TR"].items():
    assert len(row) == 3 and all(row), key

# ── Menu d'accès rapide (Windows + X) : XML valable, traduit ──────────────
for language in ("fr", "en", "es", "de"):
    out = subprocess.run([sys.executable, "-B", str(HERE / "winlux-run.py"), "menu"], capture_output=True, text=True,
                         env={**os.environ, "WINLUX_LANG": language}, check=True).stdout
    menu = ET.fromstring(out)
    labels = [item.get("label") for item in menu.iter("item")]
    commands = [command.text for command in menu.iter("command")]
    assert len(labels) >= 15 and "winlux-run" in commands and "winlux-shell-app taskmgr" in commands, language
    assert any(action.get("name") == "ToggleShowDesktop" for action in menu.iter("action"))
    if language == "de": assert "Geräte-Manager" in labels and "Ausführen" in labels
    if language == "fr": assert "Gestionnaire de périphériques" in labels

# ── Raccourcis livrés et ajoutés aux comptes existants ────────────────────
namespace = "{http://openbox.org/3.4/rc}"
shipped = ROOT / "integration/native/config/openbox/rc.xml"
keys = {item.get("key"): item for item in ET.parse(shipped).getroot().iter(f"{namespace}keybind")}
assert keys["W-i"].findtext(f".//{namespace}command") == "winlux-shell-app settings"
assert keys["W-s"].findtext(f".//{namespace}command") == "winlux-start-menu"
assert keys["W-r"].findtext(f".//{namespace}command") == "winlux-run"
assert keys["W-x"].findtext(f".//{namespace}menu") == "winlux-quick-link"
assert keys["W-p"].findtext(f".//{namespace}command") == "winlux-shell-app settings 8"
shared = ET.parse(ROOT / "integration/native/config/openbox/window-menu.xml").getroot()
assert any(menu.get("id") == "winlux-quick-link" and menu.get("execute") == "/usr/bin/winlux-run menu" for menu in shared)
source = (HERE / "winlux-native-config.py").read_text(encoding="utf-8")
tree = ast.parse(source)
added = next(node for node in tree.body if isinstance(node, ast.Assign) and getattr(node.targets[0], "id", "") == "ADDED_KEYS")
assert {"W-i", "W-s", "W-r", "W-x", "W-p"} <= set(ast.literal_eval(added.value))
assert "'winlux-run.py:winlux-run'" in (HERE / "stage-desktop-components.sh").read_text()

# Compte existant : rc.xml d'avant et Windows + R d'XFCE déjà « winlux-start-menu ».
with tempfile.TemporaryDirectory() as folder:
    home = Path(folder)
    (home / ".config/openbox").mkdir(parents=True)
    (home / ".config/openbox/rc.xml").write_text('<?xml version="1.0"?><openbox_config xmlns="http://openbox.org/3.4/rc">'
                                                 '<keyboard><keybind key="W-e"><action name="Execute"><command>x</command></action></keybind>'
                                                 '</keyboard><mouse/></openbox_config>')
    channel = home / ".config/xfce4/xfconf/xfce-perchannel-xml"
    channel.mkdir(parents=True)
    (channel / "xfce4-keyboard-shortcuts.xml").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n<channel name="xfce4-keyboard-shortcuts" version="1.0">'
        '<property name="commands" type="empty"><property name="custom" type="empty">'
        '<property name="&lt;Super&gt;r" type="string" value="winlux-start-menu"/>'
        '<property name="&lt;Super&gt;p" type="string" value="mon-outil"/>'
        '<property name="override" type="bool" value="true"/></property></property></channel>\n')
    env = {**os.environ, "HOME": str(home), "WINLUX_NATIVE_SOURCE": str(ROOT / "integration/native")}
    subprocess.run([sys.executable, "-B", str(HERE / "winlux-native-config.py")], env=env, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    migrated = {item.get("key") for item in ET.parse(home / ".config/openbox/rc.xml").getroot().iter(f"{namespace}keybind")}
    assert {"W-i", "W-s", "W-r", "W-x", "W-p"} <= migrated, migrated
    custom = {item.get("name"): item.get("value") for item in ET.parse(channel / "xfce4-keyboard-shortcuts.xml").getroot().iter("property")}
    assert custom["<Super>r"] == "winlux-run", custom
    assert custom["<Super>p"] == "mon-outil", "raccourci personnel conservé"
print("raccourcis Windows et boîte Exécuter (logique) : OK")

# ── Boîte réelle : saisie inconnue refusée, puis Échap ────────────────────
if not os.environ.get("DISPLAY") or not shutil.which("xdotool"):
    print("affichage ou xdotool absent : boîte Exécuter non ouverte")
    raise SystemExit(0)
with tempfile.TemporaryDirectory() as folder:
    env = {**os.environ, "HOME": folder, "WINLUX_LANG": "fr"}
    dialog = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-run.py")], env=env)
    try:
        window = ""
        for _ in range(60):
            window = subprocess.run(["xdotool", "search", "--name", "^Exécuter$"], capture_output=True, text=True).stdout.split()
            if window: break
            time.sleep(0.1)
        assert window, "boîte Exécuter non affichée"
        subprocess.run(["xdotool", "windowfocus", "--sync", window[0]], check=False)
        subprocess.run(["xdotool", "type", "--delay", "20", "introuvable-xyz"], check=True)
        subprocess.run(["xdotool", "key", "Return"], check=True)
        time.sleep(0.8)
        assert dialog.poll() is None, "saisie inconnue : la boîte doit rester ouverte avec un message"
        subprocess.run(["xdotool", "key", "Escape"], check=True)
        dialog.wait(timeout=5)
        assert not (Path(folder) / ".config/winlux/run-history").exists(), "saisie refusée gardée dans l'historique"
    finally:
        if dialog.poll() is None: dialog.kill()
print("boîte Exécuter (affichage réel) : OK")
