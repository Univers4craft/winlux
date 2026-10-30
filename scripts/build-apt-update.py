#!/usr/bin/env python3
"""Emit the native APT update asset and manifest; never publish them."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys

def build(stage, version, output):
    package = stage / "var/cache/winlux/installed.deb"
    fields = subprocess.check_output(["dpkg-deb", "-f", str(package), "Package", "Version", "Installed-Size"], text=True)
    data = dict(line.split(": ", 1) for line in fields.splitlines() if ": " in line)
    if data["Package"] != "winlux-desktop" or data["Version"] != version.removeprefix("v"):
        raise ValueError("Le paquet natif ne correspond pas à la version demandée")
    asset = output / f"winlux-desktop-{version}.deb"
    shutil.copy2(package, asset)
    digest = hashlib.sha256(asset.read_bytes()).hexdigest()
    size = int(data.get("Installed-Size", "0")) or sum(p.stat().st_size for p in (stage/"opt/winlux").rglob("*") if p.is_file()) // 1024
    text = (f"format=3\nbackend=apt\nbase=ubuntu-resolute\nversion={version}\nscope=system\naction=reboot\n"
            f"components=runtime,desktop,applications,updater,integration\nasset={asset.name}\nsha256={digest}\n"
            f"download_bytes={asset.stat().st_size}\nunpacked_kib={size}\n")
    (output / "winlux-update-manifest.txt").write_text(text)
    asset.with_name(asset.name+".sha256").write_text(f"{digest}  {asset.name}\n")
    print(f"Native Winlux update: {asset}")

if __name__ == "__main__": build(Path(sys.argv[1]), sys.argv[2], Path(sys.argv[3]))
