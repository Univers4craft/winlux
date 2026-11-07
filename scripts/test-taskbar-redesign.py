#!/usr/bin/python3
"""Barre des tâches redessinée (#703), sans affichage.

- winlux-tray status : réseau, son et batterie en un seul bouton (Réglages rapides) ;
- winlux-tray search : zone « Rechercher » traduite, loupe seule au choix ou
  sous 1500 px ;
- winlux-tray notifications : pastille du nombre de notifications ;
- icônes et horloge claires ou foncées selon le thème du compte ;
- winlux-personalize : couleurs de la barre dans le thème du compte (clair,
  sombre, accent, repères sous les fenêtres), style flottant, réglages
  taskbar-style / search ;
- winlux-native-config : migration unique d'une ancienne barre, dans l'ordre
  exact que produit winlux-control (pas de relance de la barre), anciennes
  couleurs de gtk.css retirées, thème du compte réécrit ;
- winlux-control align-* : recherche masquée retirée, barre flottante sans
  séparateur extensible ;
- Paramètres : style et recherche dans Personnalisation, en quatre langues.

La barre réelle (xfce4-panel sous Xvfb) est vérifiée par test-shell-contrast,
test-taskbar-icons, test-taskbar-boot-centering et test-taskbar-live-align.
"""
import ast
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


work = Path(tempfile.mkdtemp(prefix="winlux-barre-"))


def tray(kind, **env):
    home = env.pop("home", work / "home")
    result = subprocess.run([sys.executable, "-B", str(HERE / "winlux-tray.py"), kind], capture_output=True, text=True,
                            env={**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"),
                                 "XDG_STATE_HOME": str(home / ".local/state"), "WINLUX_TRAY_DIR": str(work / "tray"),
                                 "WINLUX_LANG": "fr", **env}, timeout=30)
    if result.returncode:
        print(result.stderr)
    image = re.search(r"<img>(.*?)</img>", result.stdout)
    return result.stdout, Path(image[1]) if image else None


# ── 1. Réseau + son + batterie : un seul bouton ─────────────────────────────
power = work / "power"
(power / "BAT0").mkdir(parents=True)
(power / "BAT0/capacity").write_text("64\n")
(power / "BAT0/status").write_text("Discharging\n")
output, icon = tray("status", WINLUX_POWER_SUPPLY=str(power), WINLUX_PANEL_THEME="dark")
svg = icon.read_text() if icon else ""
ok("<click>/usr/bin/winlux-popup quick</click>" in output, "état : un clic ouvre les Réglages rapides")
ok(svg.count("<g transform=") == 3 and 'width="84"' in svg, "état : réseau, son et batterie dans une seule image (84 px)")
tooltip = re.search(r"<tool>(.*?)</tool>", output, re.S)[1]
ok("Batterie : 64 %" in tooltip and tooltip.count("\n") == 2, f"état : une ligne d'infobulle par élément ({tooltip!r})")
(work / "nobattery").mkdir()
output, icon = tray("status", WINLUX_POWER_SUPPLY=str(work / "nobattery"), WINLUX_PANEL_THEME="dark")
svg = icon.read_text() if icon else ""
ok(svg.count("<g transform=") == 2 and "Batterie" not in output, "état : sans batterie (PC fixe), rien à sa place")

# ── 2. Recherche : zone, loupe, langue, petit écran ─────────────────────────
fake = work / "bin"
fake.mkdir()
(fake / "xdpyinfo").write_text("#!/bin/sh\necho \"  dimensions:    ${FAKE_WIDTH:-1920}x1080 pixels\"\n")
(fake / "xdpyinfo").chmod(0o755)
path = f"{fake}:{os.environ['PATH']}"
output, icon = tray("search", PATH=path, WINLUX_PANEL_THEME="dark")
svg = icon.read_text() if icon else ""
ok(">Rechercher</text>" in svg and 'width="196"' in svg and "<click>/usr/bin/winlux-start-menu</click>" in output,
   "recherche : zone « Rechercher » de 196 px, ouvre la recherche")
ok("<tool>Rechercher (Windows + S)</tool>" in output, "recherche : infobulle avec le raccourci")
output, icon = tray("search", PATH=path, WINLUX_PANEL_THEME="dark", WINLUX_LANG="de")
ok(icon is not None and ">Suchen</text>" in icon.read_text(), "recherche : traduite (allemand)")
output, icon = tray("search", PATH=path, WINLUX_PANEL_THEME="dark", FAKE_WIDTH="1366")
ok(icon is not None and 'width="24"' in icon.read_text(), "recherche : loupe seule sous 1500 px (portable 1366x768)")
home = work / "home-icon"
(home / ".config/winlux").mkdir(parents=True)
(home / ".config/winlux/taskbar.conf").write_text("STYLE=classic\nSEARCH=icon\n")
output, icon = tray("search", PATH=path, WINLUX_PANEL_THEME="dark", home=home)
ok(icon is not None and 'width="24"' in icon.read_text(), "recherche : « Icône seule » choisie dans Personnalisation")

# ── 3. Cloche : pastille du nombre de notifications ─────────────────────────
state = work / "home/.local/state/winlux"
state.mkdir(parents=True)
(state / "notifications.json").write_text(json.dumps([{"id": n} for n in range(3)]))
output, icon = tray("notifications", WINLUX_PANEL_THEME="dark")
ok(icon is not None and ">3</text>" in icon.read_text() and "<click>/usr/bin/winlux-popup notifications</click>" in output,
   "cloche : pastille « 3 »")
(state / "notifications.json").write_text(json.dumps([{"id": n} for n in range(12)]))
output, icon = tray("notifications", WINLUX_PANEL_THEME="dark")
ok(icon is not None and ">9+</text>" in icon.read_text(), "cloche : « 9+ » au-delà de neuf")
(state / "notifications.json").write_text("[]")
output, icon = tray("notifications", WINLUX_PANEL_THEME="dark")
ok(icon is not None and "<text" not in icon.read_text(), "cloche : pas de pastille sans notification")

# ── 4. Barre claire ou sombre selon le thème du compte ──────────────────────
for theme, expected in (("Winlux-light-2", "l"), ("Winlux-dark-1", "d"), ("Adwaita", "l"), ("HighContrastInverse", "d")):
    home = work / f"home-{theme}"
    (home / ".config/gtk-3.0").mkdir(parents=True)
    (home / ".config/gtk-3.0/settings.ini").write_text(f"[Settings]\ngtk-theme-name={theme}\n")
    output, icon = tray("notifications", home=home)
    ok(icon is not None and icon.name.endswith(f"-{expected}.svg"), f"thème {theme} : icônes {'claires' if expected == 'd' else 'foncées'}")
    clock = subprocess.run(["/bin/sh", str(HERE / "winlux-clock.sh")], capture_output=True, text=True,
                           env={**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config")}).stdout
    ok(('foreground="#f3f0fa"' if expected == "d" else 'foreground="#1d1a24"') in clock, f"thème {theme} : couleur de l'heure")

# ── 5. Couleurs de la barre dans le thème du compte ─────────────────────────
os.environ["HOME"] = str(work / "home-personalize")
personalize = runpy.run_path(str(HERE / "winlux-personalize.py"), run_name="winlux_personalize")
dark_css = personalize["panel_css"](True, "#6a3df0")
light_css = personalize["panel_css"](False, "#6a3df0")
ok(".xfce4-panel {background-color:#1c1a26" in dark_css and ".xfce4-panel {background-color:#f3f0f9" in light_css,
   "barre sombre en thème sombre, claire en thème clair")
ok(".tasklist button:checked" in dark_css and "linear-gradient(#6a3df0,#6a3df0)" in light_css,
   "repère de la fenêtre active à la couleur d'accent")
ok("#XfcePanelWindow.xfce4-panel" not in dark_css, "style classique : pas de capsule")
floating = personalize["panel_css"](True, "#6a3df0", True)
ok("#XfcePanelWindow.xfce4-panel" in floating and "border-radius:14px 14px 0 0" in floating,
   "style flottant : capsule à coins arrondis")
ok("panel_css(dark,accent,floating)" in (HERE / "winlux-personalize.py").read_text(), "le thème GTK du compte inclut la barre")
taskbar = Path(os.environ["HOME"]) / ".config/winlux/taskbar.conf"
taskbar.parent.mkdir(parents=True)
taskbar.write_text("STYLE=floating\nSEARCH=hidden\n")
ok("#XfcePanelWindow.xfce4-panel" in personalize["appearance_css"](True, "#6a3df0"), "taskbar.conf : style flottant appliqué au thème")
ok(personalize["taskbar_preferences"]() == {"STYLE": "floating", "SEARCH": "hidden"}, "taskbar.conf relu")
taskbar.write_text("STYLE=nimporte\nSEARCH=\n")
ok(personalize["taskbar_preferences"]() == {"STYLE": "classic", "SEARCH": "box"}, "valeurs inconnues ignorées")
source = (HERE / "winlux-personalize.py").read_text()
ok('kind in ("taskbar-style","search")' in source and '("taskbar-style", "classic"), ("search", "box")' in source,
   "winlux-personalize taskbar-style / search, remis d'origine par « Rétablir »")
shipped_css = (ROOT / "integration/native/config/gtk-3.0/gtk.css").read_text()
block = shipped_css[shipped_css.index("/* Winlux panel */"):shipped_css.index("/* End Winlux panel */")]
rules = re.sub(r"/\*.*?\*/", "", block, flags=re.S)
ok(not re.search(r"#[0-9a-fA-F]{3,6}\b|rgba?\(", re.sub(r"#XfcePanelWindowWrapper|#clock-button", "", rules)) and ".xfce4-panel {" not in shipped_css,
   "gtk.css livré : la forme seule, aucune couleur (elle passerait au-dessus du thème)")

# ── 6. Migration d'une barre d'avant #703 ───────────────────────────────────
config_source = (HERE / "winlux-native-config.py").read_text()
migration = {"ET": ET, "Path": Path, "os": os}
for node in ast.parse(config_source).body:
    if isinstance(node, ast.FunctionDef) and node.name in ("_plugin_command", "_set_genmon", "redesign_taskbar",
                                                           "drop_legacy_panel_css", "refresh_account_theme") \
            or isinstance(node, ast.Assign) and any(getattr(t, "id", "") == "LEGACY_PANEL_CSS" for t in node.targets):
        exec(compile(ast.Module([node], []), "winlux-native-config", "exec"), migration)
old = """<?xml version="1.0" encoding="UTF-8"?><channel name="xfce4-panel" version="1.0">
<property name="panels" type="array"><value type="int" value="1"/><property name="panel-1" type="empty">
<property name="plugin-ids" type="array">""" + "".join(f'<value type="int" value="{n}"/>' for n in (3, 13, 1, 15, 14, 2, 8, 10, 12, 4, 5, 6, 9, 7)) + """</property>
</property></property><property name="plugins" type="empty">
<property name="plugin-15" type="string" value="launcher"><property name="items" type="array"><value type="string" value="/usr/share/applications/winlux-search.desktop"/></property></property>
<property name="plugin-14" type="string" value="launcher"><property name="items" type="array"><value type="string" value="/usr/share/applications/winlux-taskview.desktop"/></property></property>
<property name="plugin-4" type="string" value="genmon"><property name="command" type="string" value="/usr/bin/winlux-tray network"/></property>
<property name="plugin-5" type="string" value="genmon"><property name="command" type="string" value="/usr/bin/winlux-tray audio"/></property>
<property name="plugin-6" type="string" value="genmon"/>
<property name="plugin-9" type="string" value="genmon"><property name="command" type="string" value="/usr/bin/winlux-tray notifications"/></property>
<property name="plugin-7" type="string" value="genmon"><property name="command" type="string" value="/usr/bin/winlux-clock"/></property>
<property name="plugin-12" type="string" value="genmon"/><property name="plugin-10" type="string" value="systray"/>
</property></channel>"""
account = work / "account"
panel_xml, markers, rc_folder = account / "xfce4-panel.xml", account / "winlux", account / "panel"
rc_folder.mkdir(parents=True)
panel_xml.write_text(old)
(rc_folder / "genmon-6.rc").write_text("Command=/usr/bin/winlux-tray battery\nUpdatePeriod=10000\n")
(rc_folder / "genmon-5.rc").write_text("Command=/usr/bin/winlux-tray audio\n")
ok(migration["redesign_taskbar"](panel_xml, markers, rc_folder), "migration : ancienne barre réécrite")
root = ET.parse(panel_xml).getroot()
ids = [value.get("value") for value in root.find(".//property[@name='plugin-ids']")]
ok(ids == "3 13 1 15 14 2 8 10 12 4 7 9".split(), f"migration : son et batterie retirés, l'heure avant la cloche ({' '.join(ids)})")
plugins = {item.get("name"): item for item in root.find(".//property[@name='plugins']")}
command = lambda name: next((c.get("value") for c in plugins[name] if c.get("name") == "command"), None)
ok("plugin-5" not in plugins and "plugin-6" not in plugins and not (rc_folder / "genmon-6.rc").exists(),
   "migration : greffons son et batterie supprimés")
ok(command("plugin-4") == "/usr/bin/winlux-tray status" and "winlux-tray status" in (rc_folder / "genmon-4.rc").read_text(),
   "migration : le genmon réseau devient l'état groupé")
ok(plugins["plugin-15"].get("value") == "genmon" and command("plugin-15") == "/usr/bin/winlux-tray search",
   "migration : le lanceur Rechercher devient la zone de recherche")
ok((markers / "taskbar-703").exists() and not migration["redesign_taskbar"](panel_xml, markers, rc_folder),
   "migration : une seule fois")

# Même ordre que winlux-control align-center : la remise en ordre de
# l'ouverture de session ne relance pas la barre.
FAKE = r'''#!/usr/bin/python3
import json, os, sys
store = os.environ["FAKE_XFCONF"]
data = json.load(open(store))
args = sys.argv[1:]
prop = args[args.index("-p") + 1]
values = [args[i + 1] for i, a in enumerate(args) if a == "-s"]
with open(os.environ["FAKE_XFCONF_LOG"], "a") as log:
    log.write(" ".join(args) + "\n")
if values:
    data[prop] = values if "-a" in args else values[0]
    json.dump(data, open(store, "w"))
    sys.exit(0)
if prop not in data:
    sys.exit(1)
value = data[prop]
print(f"Value is an array with {len(value)} items:\n\n" + "\n".join(value) if isinstance(value, list) else value)
'''
(fake / "xfconf-query").write_text(FAKE)
(fake / "xfconf-query").chmod(0o755)
for tool in ("pgrep", "xfce4-panel"):
    (fake / tool).write_text(f'#!/bin/sh\necho "{tool} $*" >> "$FAKE_XFCONF_LOG"\nexit 0\n')
    (fake / tool).chmod(0o755)
store, log = work / "xfconf.json", work / "xfconf.log"


def xfconf_from(ids_list, extra=None):
    data = {"/panels/panel-1/plugin-ids": list(ids_list), "/plugins/plugin-3": "separator", "/plugins/plugin-3/expand": "true",
            "/plugins/plugin-8": "separator", "/plugins/plugin-8/expand": "true", "/plugins/plugin-10": "systray",
            "/plugins/plugin-12": "genmon", "/plugins/plugin-14": "launcher",
            "/plugins/plugin-14/items": ["/usr/share/applications/winlux-taskview.desktop"],
            "/plugins/plugin-15": "genmon", "/plugins/plugin-15/command": "/usr/bin/winlux-tray search"}
    for number in (4, 7, 9, 13):
        data[f"/plugins/plugin-{number}"] = "genmon"
    data.update(extra or {})
    store.write_text(json.dumps(data))
    log.write_text("")


def align(how, conf=""):
    home = work / "home-align"
    (home / ".config/winlux").mkdir(parents=True, exist_ok=True)
    (home / ".config/winlux/taskbar.conf").write_text(conf)
    result = subprocess.run(["sh", str(HERE / "winlux-control.sh"), f"align-{how}"],
                            env={**os.environ, "PATH": f"{fake}:/usr/bin:/bin", "WINLUX_CONTROL_ROOT": str(work / "root"),
                                 "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"), "WINLUX_CONTROL_NO_STATUS": "1",
                                 "WINLUX_ROOT_SOCKET": str(work / "absent.sock"), "FAKE_XFCONF": str(store),
                                 "FAKE_XFCONF_LOG": str(log)}, capture_output=True, text=True, timeout=60)
    data = json.loads(store.read_text())
    return result, " ".join(data["/panels/panel-1/plugin-ids"]), data


(work / "root/etc/winlux").mkdir(parents=True)
xfconf_from(ids)
result, after, data = align("center")
ok(result.returncode == 0 and after == " ".join(ids) and "xfce4-panel -r" not in log.read_text(),
   f"barre migrée = ordre de winlux-control align-center, pas de relance ({after}) {result.stderr.strip()[:200]}")
xfconf_from(ids)
result, after, data = align("center", "STYLE=classic\nSEARCH=hidden\n")
ok(after == "3 13 1 14 2 8 10 12 4 7 9" and "xfce4-panel -r" in log.read_text(),
   f"recherche masquée : retirée de la barre, barre relancée ({after})")
xfconf_from(ids)
result, after, data = align("center", "STYLE=floating\nSEARCH=box\n")
ok(data["/plugins/plugin-3/expand"] == "false" and data["/plugins/plugin-8/expand"] == "false",
   "barre flottante : aucun séparateur extensible (elle épouse son contenu)")
xfconf_from(ids, {"/plugins/plugin-3/expand": "false", "/plugins/plugin-8/expand": "false"})
result, after, data = align("left", "STYLE=classic\nSEARCH=box\n")
ok(after == "3 1 15 14 2 8 10 12 4 7 9 13" and data["/plugins/plugin-8/expand"] == "true",
   f"retour au style classique : séparateur 8 de nouveau extensible ({after})")
xfconf_from([*"3 13 1 15 14 2 8 10 12 4 5 6 9 7".split()], {"/plugins/plugin-5": "genmon", "/plugins/plugin-6": "genmon"})
result, after, data = align("center")
ok(after == "3 13 1 15 14 2 8 10 12 4 5 6 7 9", f"compte pas encore migré : son et batterie gardés ({after})")

# Anciennes couleurs de ~/.config/gtk-3.0/gtk.css retirées, thème du compte réécrit.
style = account / "gtk.css"
style.write_text("".join(line + "\n" for line in migration["LEGACY_PANEL_CSS"]) + ".perso { color: red; }\n")
ok(migration["drop_legacy_panel_css"](style) and style.read_text().strip() == ".perso { color: red; }",
   "gtk.css du compte : anciennes couleurs de barre retirées, le reste gardé")
user = work / "home-theme"
(user / ".config/gtk-3.0").mkdir(parents=True)
(user / ".config/gtk-3.0/settings.ini").write_text("[Settings]\ngtk-theme-name=Winlux-light-2\n")
(user / ".config/winlux").mkdir(parents=True)
(user / ".config/winlux/appearance.json").write_text('{"theme": "light", "accent": "#0f7b6c"}')
theme_css = user / ".local/share/themes/Winlux-light-2/gtk-3.0/gtk.css"
theme_css.parent.mkdir(parents=True)
theme_css.write_text('@import url("x");\n/* Winlux appearance */\nwindow {color: red;}\n/* End Winlux appearance */\n')
os.environ["HOME"] = str(user)
changed = migration["refresh_account_theme"](user, HERE / "winlux-personalize.py")
text = theme_css.read_text()
ok(changed and ".xfce4-panel {background-color:#f3f0f9" in text and "linear-gradient(#0f7b6c,#0f7b6c)" in text and text.startswith('@import url("x");'),
   "thème du compte réécrit avec la barre claire et son accent")
ok(not migration["refresh_account_theme"](user, HERE / "winlux-personalize.py"), "thème déjà à jour : pas réécrit")

# ── 7. Modèle livré et Paramètres ───────────────────────────────────────────
template = ET.parse(ROOT / "integration/native/config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").getroot()
template_ids = [value.get("value") for value in template.find(".//property[@name='plugin-ids']")]
ok(template_ids == "3 1 15 14 2 8 10 4 7 9 13".split(), f"modèle : barre de #703 ({' '.join(template_ids)})")
rc = ROOT / "integration/native/config/xfce4/panel"
ok("winlux-tray status" in (rc / "genmon-4.rc").read_text() and "winlux-tray search" in (rc / "genmon-15.rc").read_text()
   and not (rc / "genmon-5.rc").exists() and not (rc / "genmon-6.rc").exists(), "modèle : genmon état et recherche")
pages = (HERE / "winlux-settings-pages.py").read_text()
ok('"winlux-personalize", "taskbar-style"' in pages and '"winlux-personalize", "search"' in pages,
   "Paramètres : style et recherche de la barre dans Personnalisation")
for label in ("Classique, sur toute la largeur", "Flottante, au centre", "Zone de recherche", "Icône seule", "Masquée",
              "Windows + S ouvre toujours la recherche, même masquée."):
    row = re.search(r'\n    "' + re.escape(label) + r'": \(("[^"]+"), ("[^"]+"), ("[^"]+")\)', pages)
    ok(row is not None, f"traduit en anglais, espagnol et allemand : « {label} »")

if failures:
    print(f"{len(failures)} échec(s)")
    raise SystemExit(1)
print("OK: barre des tâches redessinée (#703)")
