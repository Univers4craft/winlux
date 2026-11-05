#!/usr/bin/python3
"""Ancrage Windows + flèches / Windows + Z et vue des tâches Windows + Tab (logique et raccourcis)."""
import ast
from pathlib import Path
import runpy
import shutil
import sys
import tempfile
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
snap = runpy.run_path(str(HERE / "winlux-snap.py"), run_name="snap_test")
view = runpy.run_path(str(HERE / "winlux-taskview.py"), run_name="taskview_test")

# Windows + flèches, comme Windows 11.
step = snap["next_state"]
assert step("", "left") == ("snap", "half:left") and step("half:right", "left") == ("restore", None)
assert step("half:left", "up") == ("snap", "quarter:tl") and step("quarter:bl", "up") == ("snap", "half:left")
assert step("half:right", "down") == ("snap", "quarter:br") and step("quarter:tr", "down") == ("snap", "half:right")
assert step("", "up") == ("maximize", None) and step("", "down", maximized=True) == ("restore", None)
assert step("", "down") == ("minimize", None) and step("quarter:tl", "up") == ("none", None)
# Zones : la zone de travail est remplie sans trou ni chevauchement.
area = (0, 40, 1366, 728)
rect = snap["zone_rect"]
for layout in snap["LAYOUTS"]:
    cells = [rect(zone, area) for zone in layout]
    assert sum(w * h for _, _, w, h in cells) == area[2] * area[3], layout
    for zone in layout: assert zone in snap["ZONES"]
assert rect("half:right", area) == (683, 40, 683, 728)
assert rect("third:3", (0, 0, 1000, 600)) == (667, 0, 333, 600)
# Cadre d'openbox retiré de la taille demandée ; écran choisi selon le centre de la fenêtre.
assert snap["client_geometry"]((0, 0, 683, 728), (1, 1, 23, 4)) == (0, 0, 681, 701)
assert snap["pick_area"]([(0, 0, 1920, 1040), (1920, 0, 1280, 1024)], (2500, 300)) == (1920, 0, 1280, 1024)
assert snap["window_number"]("0x00400003") == snap["window_number"](str(0x400003))
assert set(snap["OPPOSITE"]) <= set(snap["ZONES"])

# Bureaux virtuels : suppression d'un bureau, noms par défaut traduits.
moves = view["removal_moves"](3, 1, [("a", 0), ("b", 1), ("c", 2)])
assert moves == [("b", 0), ("c", 1)], moves
assert view["removal_moves"](2, 0, [("a", 0), ("b", 1)]) == [("a", 0), ("b", 0)]
name = view["display_name"]
assert name(1, "desktop 2") == view["_t"]("Bureau {n}").format(n=2) and name(0, "Bureau Winlux").endswith("1")
assert name(2, "Travail") == "Travail"
# openbox nomme les nouveaux bureaux dans la langue du système (VM : « Espace de travail 2 »).
for automatic in ("Espace de travail 2", "Workspace 2", "Escritorio 2", "Arbeitsfläche 2"):
    assert name(1, automatic) == view["_t"]("Bureau {n}").format(n=2), automatic
for module in (snap, view):
    for key, row in module["_TR"].items():
        assert len(row) == 3 and all(row), key

# Raccourcis livrés, et mise à jour d'un rc.xml d'avant (actions openbox fixes remplacées).
namespace = "{http://openbox.org/3.4/rc}"
shipped = HERE.parent / "integration/native/config/openbox/rc.xml"


def bindings(path):
    root = ET.parse(path).getroot()
    return {item.get("key"): [(action.get("name"), action.findtext(f"{namespace}command"))
                              for action in item.findall(f"{namespace}action")] for item in root.iter(f"{namespace}keybind")}


keys = bindings(shipped)
assert keys["W-Left"] == [("Execute", "winlux-snap key left")] and keys["W-z"] == [("Execute", "winlux-snap layouts")]
assert keys["W-Tab"] == [("Execute", "winlux-taskview")] and keys["W-C-d"] == [("Execute", "winlux-taskview new")]
assert keys["W-C-Right"] == [("GoToDesktop", None)] and keys["W-S-Right"] == [("MoveResizeTo", None)]
source = (HERE / "winlux-native-config.py").read_text(encoding="utf-8")
namespace_globals = {"ET": ET, "sys": sys, "_t": lambda text: text}
for node in ast.parse(source).body:
    if (isinstance(node, ast.Assign) and any(getattr(t, "id", "") in ("ADDED_KEYS", "REPLACED_KEYS", "OLD_SNAP_ACTIONS") for t in node.targets)) \
            or (isinstance(node, ast.FunctionDef) and node.name == "migrate_openbox"):
        exec(compile(ast.Module([node], []), "winlux-native-config", "exec"), namespace_globals)
old = """<?xml version="1.0"?><openbox_config xmlns="http://openbox.org/3.4/rc"><keyboard>
<keybind key="W-Left"><action name="UnmaximizeFull"/><action name="MoveResizeTo"><x>0</x></action></keybind>
<keybind key="W-Up"><action name="Maximize"/></keybind>
<keybind key="W-C-Left"><action name="UnmaximizeFull"/><action name="MoveResizeTo"><x>0</x></action></keybind>
<keybind key="W-Right"><action name="Execute"><command>mon-script</command></action></keybind>
</keyboard><mouse><context name="Maximize"><mousebind button="Left" action="Click"><action name="ToggleMaximize"/></mousebind></context></mouse></openbox_config>"""
with tempfile.TemporaryDirectory() as folder:
    user = Path(folder) / "rc.xml"
    user.write_text(old)
    namespace_globals["migrate_openbox"](user, shipped)
    migrated = bindings(user)
    assert migrated["W-Left"] == keys["W-Left"] and migrated["W-Up"] == keys["W-Up"] and migrated["W-C-Left"] == keys["W-C-Left"]
    assert migrated["W-Right"] == [("Execute", "mon-script")], "raccourci personnel conservé"
    for added in ("W-Tab", "W-z", "W-v", "W-period", "W-C-d", "W-C-F4"):
        assert added in migrated, added
    maximize = [item.get("button") for item in ET.parse(user).getroot().iter(f"{namespace}mousebind")]
    assert maximize == ["Left", "Right"], maximize
    before = user.read_bytes()
    namespace_globals["migrate_openbox"](user, shipped)
    assert user.read_bytes() == before, "deuxième passage sans changement"
    # Compte créé avant les touches de luminosité : elles arrivent à la session suivante.
    assert migrated.get("XF86MonBrightnessUp") == [("Execute", "winlux-brightness key up")] \
        or bindings(user).get("XF86MonBrightnessUp") == [("Execute", "winlux-brightness key up")], "touches de luminosité ajoutées"
    # Supprimé ensuite par l'utilisateur : ne revient pas.
    tree = ET.parse(user)
    keyboard = tree.getroot().find(f"{namespace}keyboard")
    for binding in list(keyboard):
        if binding.get("key") == "XF86MonBrightnessDown": keyboard.remove(binding)
    tree.write(user)
    namespace_globals["migrate_openbox"](user, shipped)
    assert "XF86MonBrightnessDown" not in bindings(user), "raccourci supprimé par l'utilisateur rétabli"
# Bureaux virtuels (#459) : fenêtre emmenée sur le bureau voisin, bouton
# Vue des tâches après Démarrer, barre limitée au bureau en cours.
assert keys["W-C-S-Left"] == [("SendToDesktop", None)] and keys["W-C-S-Right"] == [("SendToDesktop", None)]
right = ET.parse(shipped).getroot().find(f".//{namespace}keybind[@key='W-C-S-Right']")
assert right.findtext(f".//{namespace}follow") == "yes" and right.findtext(f".//{namespace}to") == "right"
assert {"W-C-S-Left", "W-C-S-Right"} <= set(namespace_globals["ADDED_KEYS"])
panel_file = HERE.parent / "integration/native/config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml"
panel_root = ET.parse(panel_file).getroot()
order = [value.get("value") for value in panel_root.find(".//property[@name='plugin-ids']")]
assert order[order.index("1") + 1] == "14", order
assert panel_root.find(".//property[@name='plugin-14']/property[@name='items']/value").get("value") \
    == "/usr/share/applications/winlux-taskview.desktop"
assert panel_root.find(".//property[@name='plugin-2']/property[@name='all-workspaces']").get("value") == "false"
desktop_file = (HERE.parent / "integration/native/applications/winlux-taskview.desktop").read_text(encoding="utf-8")
for language in ("en", "es", "de"):
    assert f"Name[{language}]=" in desktop_file and f"Comment[{language}]=" in desktop_file, language
assert "Exec=/usr/bin/winlux-taskview\n" in desktop_file and (HERE.parent / "integration/native/taskview.svg").is_file()
migration = {"ET": ET}
for node in ast.parse(source).body:
    if (isinstance(node, ast.Assign) and any(getattr(t, "id", "") == "TASKBAR_BUTTONS" for t in node.targets)) \
            or (isinstance(node, ast.FunctionDef) and node.name == "add_taskbar_buttons"):
        exec(compile(ast.Module([node], []), "winlux-native-config", "exec"), migration)
old_panel = """<?xml version="1.0" encoding="UTF-8"?><channel name="xfce4-panel" version="1.0">
<property name="panels" type="array"><value type="int" value="1"/><property name="panel-1" type="empty">
<property name="plugin-ids" type="array"><value type="int" value="3"/><value type="int" value="13"/><value type="int" value="1"/>
<value type="int" value="2"/><value type="int" value="8"/><value type="int" value="14"/><value type="int" value="7"/></property></property></property>
<property name="plugins" type="empty"><property name="plugin-1" type="string" value="launcher"/>
<property name="plugin-2" type="string" value="tasklist"><property name="all-workspaces" type="bool" value="true"/></property>
<property name="plugin-14" type="string" value="launcher"/><property name="plugin-13" type="string" value="genmon"/>
<property name="plugin-3" type="string" value="separator"/><property name="plugin-8" type="string" value="separator"/>
<property name="plugin-7" type="string" value="genmon"/></property></channel>"""
with tempfile.TemporaryDirectory() as folder:
    panel_xml, markers = Path(folder) / "xfce4-panel.xml", Path(folder) / "winlux"
    panel_xml.write_text(old_panel)
    assert migration["add_taskbar_buttons"](panel_xml, markers)
    root = ET.parse(panel_xml).getroot()
    order = [value.get("value") for value in root.find(".//property[@name='plugin-ids']")]
    assert order == ["3", "13", "1", "15", "2", "8", "14", "7"], order  # 14 déjà pris (assistant…) : 15
    assert root.find(".//property[@name='plugin-15']/property[@name='items']/value").get("value").endswith("winlux-taskview.desktop")
    assert root.find(".//property[@name='all-workspaces']").get("value") == "false"
    # Bouton retiré par l'utilisateur, barre remise sur tous les bureaux : rien ne revient.
    panel_xml.write_text(old_panel)
    assert not migration["add_taskbar_buttons"](panel_xml, markers)
    assert panel_xml.read_text() == old_panel
stage = (HERE / "stage-desktop-components.sh").read_text()
assert "'winlux-snap.py:winlux-snap'" in stage and "'winlux-taskview.py:winlux-taskview'" in stage
print("ancrage et vue des tâches : OK")
