#!/usr/bin/python3
"""Repair permissions only for unchanged copies of installed launchers."""
import os
from pathlib import Path
import subprocess

desktop = Path.home() / "Bureau"
if desktop.is_dir() and not desktop.is_symlink():
    for shortcut in desktop.glob("*.desktop"):
        installed = Path("/usr/share/applications") / shortcut.name
        try:
            if shortcut.is_symlink() or not installed.is_file() or shortcut.stat().st_uid != os.geteuid():
                continue
            if shortcut.read_bytes() != installed.read_bytes():
                continue
            shortcut.chmod(shortcut.stat().st_mode | 0o100)
            subprocess.run(["gio", "set", "-t", "string", str(shortcut), "metadata::trusted", "true"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        except OSError:
            continue
