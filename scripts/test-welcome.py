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
print("OK: welcome completion saved in isolated profile")
