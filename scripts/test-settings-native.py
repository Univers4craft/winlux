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
        namespace = window.poll.__globals__
        original_read = namespace["read"]
        try:
            namespace["read"] = lambda path: "Winlux est à jour.\n" if path == "/run/winlux/updates.txt" else original_read(path)
            window.nav.select_row(window.nav.get_row_at_index(7))
            window.poll()
            assert not window.install_button.get_parent().get_visible(), "Install button shown for current version"
        finally:
            namespace["read"] = original_read
        window.show_update_error("update-apply", 1)
        dialogs = [widget for widget in Gtk.Window.list_toplevels() if isinstance(widget, Gtk.MessageDialog)]
        assert len(dialogs) == 1, "Update error dialog was not displayed"
        assert "Étape :" in dialogs[0].get_property("secondary-text")
        dialogs[0].destroy()
    except Exception as error:
        failures.append(error)
    finally:
        window.destroy()
    return False

GLib.idle_add(check)
Gtk.main()
if failures:
    raise failures[0]
print(f"OK: {len(module['PAGES'])} native settings pages, progress visibility, no system actions")
