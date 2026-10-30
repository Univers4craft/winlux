#!/usr/bin/python3
"""Check resumable local transfers and measured integrity progress."""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile

helper = Path(__file__).with_name("winlux-update-progress.py")
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    data = b"winlux update\n" * 1000
    source, partial = root / "source", root / "partial"
    source.write_bytes(data)
    partial.write_bytes(data[:321])
    env = dict(os.environ, WINLUX_PROGRESS_FILE=str(root / "progress"))
    subprocess.run(["python3", str(helper), "download", source.as_uri(), str(len(data)), str(partial)], check=True, env=env)
    assert partial.read_bytes() == data
    assert "percent=100\n" in (root / "progress").read_text()
    digest = subprocess.check_output(["python3", str(helper), "hash", str(partial), "0"], env=env, text=True).strip()
    assert digest == hashlib.sha256(data).hexdigest()
    assert "phase=verifying\n" in (root / "progress").read_text()
print("OK: resumable transfer and measured SHA-256")
