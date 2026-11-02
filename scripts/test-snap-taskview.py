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
stage = (HERE / "stage-desktop-components.sh").read_text()
assert "'winlux-snap.py:winlux-snap'" in stage and "'winlux-taskview.py:winlux-taskview'" in stage
print("ancrage et vue des tâches : OK")
