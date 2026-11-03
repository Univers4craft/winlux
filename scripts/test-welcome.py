#!/usr/bin/python3
"""Verify onboarding records completion without running system actions."""
import os
from pathlib import Path
import runpy
import tempfile

with tempfile.TemporaryDirectory() as directory:
    os.environ["HOME"] = directory
    module = runpy.run_path(str(Path(__file__).with_name("winlux-welcome.py")), run_name="welcome_test")
    window = module["Welcome"]()
    marker = Path(directory) / ".config/winlux/welcome-complete"
    assert not marker.exists()
    def finish():
        window.finish()
        return False
    module["GLib"] = __import__("gi.repository", fromlist=["GLib"]).GLib
    module["GLib"].idle_add(finish)
    window.show_all()
    module["Gtk"].main()
    assert marker.read_text() == "1\n"
# Menu Démarrer : la fenêtre se rouvre après « Terminer », dans les quatre
# langues, sous le titre de la fenêtre elle-même.
import configparser
entry = configparser.RawConfigParser(strict=False, interpolation=None)
entry.optionxform = str
entry.read(Path(__file__).resolve().parent.parent / "integration/native/applications/winlux-welcome.desktop", encoding="utf-8")
values = entry["Desktop Entry"]
assert values["Exec"] == "/usr/bin/winlux-welcome" and values.get("NoDisplay", "false") != "true"
titles = module["_TR"]["Bienvenue dans Winlux"]
assert values["Name"] == "Bienvenue dans Winlux"
assert (values["Name[en]"], values["Name[es]"], values["Name[de]"]) == titles, titles
assert all(values.get(f"Comment{suffix}") for suffix in ("", "[en]", "[es]", "[de]"))
print("OK: welcome completion saved in isolated profile, reopened from the Start menu in 4 languages")
