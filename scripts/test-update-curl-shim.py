#!/usr/bin/python3
"""Test VM only: answers the GitHub release URLs read by Winlux's updater from local files."""
import hashlib
from pathlib import Path
import shutil
import sys

bench = Path("/opt/winlux-test/update")
manifest = dict(line.split("=", 1) for line in (bench / "winlux-update-manifest.txt").read_text().splitlines() if "=" in line)
args = sys.argv[1:]
output = args[args.index("--output") + 1] if "--output" in args else None
url = next(argument for argument in reversed(args) if argument.startswith("https://"))
name = url.rsplit("/", 1)[-1]
asset = manifest["asset"]
if "/releases/" not in url or name not in ("winlux-update-manifest.txt", asset, asset + ".sha256"):
    sys.stderr.write(f"curl: (22) The requested URL returned error: 404 {url}\n")
    sys.exit(22)
if name.endswith(".sha256"):
    data = f"{hashlib.sha256((bench / asset).read_bytes()).hexdigest()}  {asset}\n".encode()
else:
    data = (bench / name).read_bytes()
if output: Path(output).write_bytes(data)
else: sys.stdout.buffer.write(data)
