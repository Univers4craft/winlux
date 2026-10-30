#!/usr/bin/python3
"""Exercise icon selection with an isolated profile and mocked XFCE."""
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
from unittest.mock import patch
from gi.repository import GdkPixbuf

with tempfile.TemporaryDirectory() as directory:
    with patch.dict(os.environ, {"HOME": directory}):
        module = runpy.run_path(str(Path(__file__).with_name("winlux-personalize.py")))
        image = Path(directory) / "chosen.png"
        pixbuf = GdkPixbuf.Pixbuf.new(GdkPixbuf.Colorspace.RGB, True, 8, 8, 8)
        pixbuf.fill(0xff00ffff)
        pixbuf.savev(str(image), "png", [], [])
        with patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 0)):
            module["apply"]("start-icon", str(image))
            launcher = Path(directory) / ".local/share/winlux/start/winlux-start.desktop"
            assert "Exec=/usr/bin/winlux-start-menu" in launcher.read_text()
            assert "Icon=" + str(launcher.parent / "icon.png") in launcher.read_text()
            module["apply"]("start-icon", "default")
            assert "Icon=/usr/share/winlux/native/start.svg" in launcher.read_text()
print("OK: custom Start icon and default restoration")
