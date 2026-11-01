#!/usr/bin/python3
"""Offline checks: no native installation marker, network or package writes."""
from pathlib import Path
import runpy
import tempfile

directory = Path(__file__).resolve().parent
updater = runpy.run_path(str(directory / "winlux-update-apt.py"), run_name="apt_metadata_test")
builder = runpy.run_path(str(directory / "stage-apt-root.py"), run_name="apt_payload_test")
valid = ("format=3\nbackend=apt\nbase=ubuntu-resolute\nversion=v0.3.0\n"
         "asset=winlux-desktop-v0.3.0.deb\nsha256=" + "a" * 64 + "\n"
         "download_bytes=123\nunpacked_kib=456\n")
assert updater["manifest"](valid)["version"] == "v0.3.0"
for invalid in [valid + "format=3\n", valid.replace("format=3", "format=2"),
                valid.replace("ubuntu-resolute", "other-base"), valid.replace("asset=winlux", "asset=../winlux"),
                valid.replace("download_bytes=123", "download_bytes=-1"), valid.replace("a"*64, "not-a-hash")]:
    try: updater["manifest"](invalid)
    except ValueError: pass
    else: raise AssertionError("Incompatible or malformed manifest accepted")
for path in ("usr/bin/telnet", "usr/lib/x86_64-linux-gnu/libc.so.6", "usr/lib/modules/old/driver.ko", "usr/share/winlux/kernel-version"):
    assert not builder["winlux_path"](path), path
for path in ("usr/bin/winlux", "usr/share/winlux/native/config/panel.xml", "usr/share/applications/winlux-settings.desktop",
             "usr/share/thumbnailers/winlux-windows.thumbnailer"):
    assert builder["winlux_path"](path), path
with tempfile.TemporaryDirectory(prefix="apt-cache-check.") as temporary:
    root = Path(temporary); source = root / "source"; target = root / "installed.deb"
    source.write_bytes(b"new package"); target.write_bytes(b"previous package")
    updater["cache_copy"](source, target)
    assert target.read_bytes() == b"new package"
    assert not list(root.glob(".package-*"))
print("OK: native manifest compatibility, package ownership boundaries and atomic cache replacement")
