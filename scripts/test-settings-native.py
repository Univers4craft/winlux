#!/usr/bin/python3
"""Exercise settings navigation without launching system actions."""
import runpy
from pathlib import Path

module = runpy.run_path(str(Path(__file__).with_name("winlux-settings.py")), run_name="settings_test")
Gtk, GLib = module["Gtk"], module["GLib"]
window = module["Settings"]()
failures = []

def check():
    try:
        for index, title in enumerate(module["PAGES"]):
            window.nav.select_row(window.nav.get_row_at_index(index))
            assert window.page == index, title
            assert window.body.get_children()[0].get_text() == title
            assert window.progress.get_visible() == (index == 7)
        assert window.process is None, "Navigation launched a system action"
    except Exception as error:
        failures.append(error)
    finally:
        window.destroy()
    return False

GLib.idle_add(check)
Gtk.main()
if failures:
    raise failures[0]
print("OK: 14 native settings pages, progress visibility, no system actions")
