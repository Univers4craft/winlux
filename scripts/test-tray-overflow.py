#!/usr/bin/python3
"""Icônes cachées derrière la flèche de la zone de notification, une par programme.

Demande de damien (8 novembre 2026) : ranger une icône de la zone de
notification derrière la flèche, à côté du réseau, et l'en ressortir.

1. Toutes les icônes des programmes Windows (winlux-notify-icon) portaient le
   même nom pour le greffon systray d'XFCE, « winlux-notify-icon.py » : les
   cacher une à une était impossible. Chaque assistant prend désormais le nom
   de son .exe (putty, filezilla).
2. `winlux-tray overflow list|hide|show` lit et écrit hidden-legacy-items
   (XEmbed) et hidden-items (StatusNotifier), sans toucher nm-applet.
   Clic droit : le menu du programme (TrackPopupMenu du runtime) reçoit
   « Ranger dans les icônes cachées » (tests Rust de popup.rs) ; un programme
   sans menu, ou Maj + clic droit, ouvre le petit menu de Winlux.
3. Paramètres > Personnalisation : « Autres icônes de la barre d’état
   système », un interrupteur par icône, en quatre langues.
4. Sous X (xvfb-run) avec xfce4-panel : deux programmes, deux noms retenus par
   la vraie barre ; PuTTY caché en direct, la flèche apparaît, FileZilla reste.
"""
import ast
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


# ── 1. Nom propre à chaque programme ────────────────────────────────────────
source = (HERE / "winlux-notify-icon.py").read_text(encoding="utf-8")
tree = ast.parse(source)
scope = {"Path": Path}
exec(compile(ast.Module(body=[node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "program_name"],
                        type_ignores=[]), "winlux-notify-icon.py", "exec"), scope)
name = scope["program_name"]
ok(name("/opt/apps/putty.exe") == "putty" and name("C:\\Programmes\\FileZilla.exe") == "filezilla"
   and name("Mon Appli (x86).exe") == "mon-appli--x86" and name("") == "winlux-programme",
   "nom de l'icône = nom du .exe en minuscules, chemin Windows compris")
ok('elif command == "menu"' in source and "SHIFT_MASK" in source
   and all(f'"{lang}": "' in source for lang in ("fr", "en", "es", "de")),
   "menu de Winlux : programme sans menu ou Maj + clic droit, en quatre langues")
popup = (HERE.parent / "runtime/src/dlls/controls/popup.rs").read_text(encoding="utf-8")
ok("take_tray_menu()" in popup and "menus::truncate(menu, kept)" in popup,
   "menu du programme : entrée ajoutée après un clic droit sur son icône, retirée à la fermeture")
ok("GLib.set_prgname(program_name(executable))" in source and source.index("set_prgname") < source.index("Gtk.StatusIcon()"),
   "nom posé avant la première icône (la zone de notification le lit à la création)")

# ── 2. winlux-tray overflow, sur un xfconf simulé ──────────────────────────
FAKE = r'''#!/usr/bin/python3
import json, os, sys
store = os.environ["FAKE_XFCONF"]
data = json.load(open(store))
args = sys.argv[1:]
prop = args[args.index("-p") + 1]
if "-r" in args:
    data.pop(prop, None); json.dump(data, open(store, "w")); sys.exit(0)
values = [args[i + 1] for i, a in enumerate(args) if a == "-s"]
if values:
    if prop not in data and "-n" not in args:
        sys.exit(1)
    data[prop] = values
    json.dump(data, open(store, "w"))
    sys.exit(0)
if prop not in data:
    sys.exit(1)
print(f"Value is an array with {len(data[prop])} items:\n")
print("\n".join(data[prop]))
'''
with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    (work / "bin").mkdir()
    fake = work / "bin/xfconf-query"
    fake.write_text(FAKE)
    fake.chmod(0o755)
    store = work / "xfconf.json"
    store.write_text(json.dumps({
        "/plugins/plugin-10/known-legacy-items": ["winlux-notify-icon.py", "putty", "filezilla"],
        "/plugins/plugin-10/known-items": ["nm-applet", "blueman"],
        "/plugins/plugin-10/hidden-items": ["nm-applet"]}))
    env = dict(os.environ, PATH=f"{work}/bin:/usr/bin:/bin", FAKE_XFCONF=str(store), HOME=str(work),
               XDG_CACHE_HOME=str(work / "cache"), WINLUX_TRAY_DIR=str(work / "tray"))

    def tray(*args):
        return subprocess.run([sys.executable, "-B", str(HERE / "winlux-tray.py"), "overflow", *args], env=env,
                              capture_output=True, text=True, timeout=30)

    listing = json.loads(tray("list").stdout or "[]")
    ok([(item["name"], item["hidden"]) for item in listing] == [("putty", False), ("filezilla", False), ("blueman", False)],
       f"liste : un interrupteur par programme, sans nm-applet ni l'ancien nom commun ({listing})")
    result = tray("hide", "putty")
    data = json.loads(store.read_text())
    ok(result.returncode == 0 and data.get("/plugins/plugin-10/hidden-legacy-items") == ["putty"]
       and data["/plugins/plugin-10/hidden-items"] == ["nm-applet"],
       f"PuTTY caché (icône XEmbed), nm-applet toujours caché ({data})")
    tray("hide", "blueman")
    data = json.loads(store.read_text())
    ok(data["/plugins/plugin-10/hidden-items"] == ["nm-applet", "blueman"], "icône StatusNotifier cachée aussi")
    tray("show", "putty")
    data = json.loads(store.read_text())
    ok("/plugins/plugin-10/hidden-legacy-items" not in data, "PuTTY ressorti dans la barre")
    ok(tray("hide", "nm-applet").returncode == 2 and tray("hide", "inconnu").returncode == 1,
       "nm-applet non proposé, icône inconnue refusée")

# ── 3. Paramètres en quatre langues ────────────────────────────────────────
pages = (HERE / "winlux-settings-pages.py").read_text(encoding="utf-8")
ok("def tray_icon_settings" in pages and '"winlux-tray", "overflow", "show" if on else "hide"' in pages
   and "self.tray_icon_settings()" in pages, "Personnalisation : un interrupteur par icône")
for text in ("Autres icônes de la barre d’état système",
             "Aucune pour l’instant : chaque programme qui affiche une icône près de l’heure apparaît ici.",
             "Désactivée, l’icône se range derrière la flèche, à côté du réseau. Un clic sur la flèche la montre."):
    ok(f"    '{text}': (" in pages, f"traduit en anglais, espagnol et allemand : {text[:40]}…")

# ── 4. Vraie barre ──────────────────────────────────────────────────────────
missing = [tool for tool in ("xfce4-panel", "openbox", "dbus-run-session", "xfconf-query") if not shutil.which(tool)]
if missing or not os.environ.get("DISPLAY"):
    print("Barre réelle non vérifiée : " + (", ".join(missing) or "pas d'affichage (lancer sous xvfb-run)"))
else:
    import importlib.util
    spec = importlib.util.spec_from_file_location("live", HERE / "test-taskbar-live-align.py")
    live = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(live)
    live.MARK = ""
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temp:
        home = Path(temp) / "compte"
        live.prepare(home)
        env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"), XDG_CACHE_HOME=str(home / ".cache"),
                   WINLUX_TRAY_DIR=str(home / "tray"), WINLUX_LANG="fr")
        notify, tray_script = HERE / "winlux-notify-icon.py", HERE / "winlux-tray.py"
        script = f"""openbox & sleep 1; xfce4-panel --disable-wm-check & sleep 3
( printf 'set\\tk1\\t/opt/apps/putty.exe\\tPuTTY\\t1\\n'; sleep 10; printf 'menu\\tk1\\n'; sleep 30 ) | {sys.executable} -B {notify} >/dev/null 2>&1 &
( printf 'set\\tk2\\tC:\\\\Programmes\\\\FileZilla.exe\\tFileZilla\\t1\\n'; sleep 30 ) | {sys.executable} -B {notify} >/dev/null 2>&1 &
sleep 5
{sys.executable} -B {tray_script} overflow list > "{temp}/avant.json"
pid=$(pgrep -nx xfce4-panel)
{sys.executable} -B {tray_script} overflow hide putty
sleep 2
{sys.executable} -B {tray_script} overflow list > "{temp}/apres.json"
test "$(pgrep -nx xfce4-panel)" = "$pid" && echo meme > "{temp}/barre"
{sys.executable} -B {tray_script} overflow show putty
sleep 5
# Programme sans menu : l'assistant ouvre le menu de Winlux ; Entrée choisit
# « Ranger dans les icônes cachées ».
xdotool key Down Return 2>/dev/null
sleep 2
{sys.executable} -B {tray_script} overflow list > "{temp}/menu.json"
xfce4-panel -q; kill 0"""
        subprocess.run(["timeout", "-k", "3", "60", "dbus-run-session", "--", "sh", "-c", script], env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        before = json.loads((Path(temp) / "avant.json").read_text() or "[]") if (Path(temp) / "avant.json").exists() else []
        after = json.loads((Path(temp) / "apres.json").read_text() or "[]") if (Path(temp) / "apres.json").exists() else []
        ok(sorted(item["name"] for item in before) == ["filezilla", "putty"],
           f"vraie barre : PuTTY et FileZilla retenus séparément ({before})")
        ok({item["name"]: item["hidden"] for item in after} == {"putty": True, "filezilla": False}
           and (Path(temp) / "barre").exists(),
           f"vraie barre : PuTTY caché en direct, FileZilla reste, barre non relancée ({after})")
        menu = json.loads((Path(temp) / "menu.json").read_text() or "[]") if (Path(temp) / "menu.json").exists() else []
        if shutil.which("xdotool"):
            ok({item["name"]: item["hidden"] for item in menu} == {"putty": True, "filezilla": False},
               f"vraie barre : menu de Winlux sur l'icône, « Ranger dans les icônes cachées » ({menu})")

# ── 5. Infobulle retirée au clic (#1371) ────────────────────────────────────
class FakeIcon:
    def __init__(self): self.has_tooltip, self.text, self.on = True, "claude", True
    def set_has_tooltip(self, value): self.has_tooltip = value
    def set_tooltip_text(self, text): self.text, self.has_tooltip = text, True

class FakeGLib:
    pending = []
    @staticmethod
    def timeout_add(_ms, fn, *args): FakeGLib.pending.append((fn, args))

scope = {"GLib": FakeGLib, "ICONS": {}, "TIPS": {}, "QUIET": set()}
exec(compile(ast.Module(body=[n for n in tree.body if isinstance(n, ast.FunctionDef)
                              and n.name in ("quiet_tooltip", "tooltip_back")], type_ignores=[]),
             "winlux-notify-icon.py", "exec"), scope)
icon = FakeIcon()
scope["ICONS"]["k"], scope["TIPS"]["k"] = icon, "claude"
scope["pointer_on"] = lambda i: i.on
scope["quiet_tooltip"]("k")
ok(not icon.has_tooltip and "k" in scope["QUIET"] and len(FakeGLib.pending) == 1,
   "clic sur l'icône : infobulle retirée, retour programmé (#1371)")
fn, args = FakeGLib.pending[0]
ok(fn(*args) is True and not icon.has_tooltip, "pointeur encore sur l'icône : l'infobulle reste cachée")
icon.on = False
ok(fn(*args) is False and icon.has_tooltip and icon.text == "claude" and "k" not in scope["QUIET"],
   "pointeur sorti de l'icône : l'infobulle revient avec son texte")
ok("quiet_tooltip(key)" in source and "if key not in QUIET:" in source,
   "assistant : retrait au clic, texte mis de côté pendant le menu")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("OK: icônes cachées une par une derrière la flèche de la zone de notification")
