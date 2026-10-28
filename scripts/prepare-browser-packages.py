#!/usr/bin/env python3
"""Cache official amd64 browser DEBs; verify publisher-index SHA256 hashes."""
import gzip
import hashlib
import pathlib
import re
import subprocess
import sys
import tempfile
import urllib.parse

SOURCES = {
    "edge": ("microsoft-edge-stable", "https://packages.microsoft.com/repos/edge/", "dists/stable/main/binary-amd64/Packages.gz"),
    "chrome": ("google-chrome-stable", "https://dl.google.com/linux/chrome/deb/", "dists/stable/main/binary-amd64/Packages.gz"),
    "firefox": ("firefox", "https://packages.mozilla.org/apt/", "dists/mozilla/main/binary-amd64/Packages"),
}

def fetch(url, destination):
    subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error",
                    "--connect-timeout", "15", "--max-time", "300", "--max-filesize", "536870912",
                    url, "--output", str(destination)], check=True)

def select(data, package):
    if data.startswith(b"\x1f\x8b"):
        data = gzip.decompress(data)
    best = None
    for stanza in data.decode("utf-8").split("\n\n"):
        fields = dict(line.split(": ", 1) for line in stanza.splitlines() if ": " in line and not line.startswith(" "))
        if fields.get("Package") != package or fields.get("Architecture") != "amd64":
            continue
        filename = fields.get("Filename", "")
        if not filename.startswith("pool/") or ".." in filename.split("/") or not re.fullmatch(r"[A-Za-z0-9_+~./:-]+\.deb", filename):
            raise ValueError("invalid package filename")
        if not re.fullmatch(r"[0-9a-f]{64}", fields.get("SHA256", "")):
            raise ValueError("invalid SHA256")
        if not re.fullmatch(r"[A-Za-z0-9.+:~_-]+", fields.get("Version", "")):
            raise ValueError("invalid version")
        if best is None or subprocess.run(["dpkg", "--compare-versions", fields["Version"], "gt", best["Version"]]).returncode == 0:
            best = fields
    if best is None:
        raise ValueError(f"package not found: {package}")
    return best

def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def main():
    output = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "build/browser-packages").resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=output, prefix=".download-") as work:
        work = pathlib.Path(work)
        for browser, (package, base, index) in SOURCES.items():
            fetch(base + index, work / "Packages.gz")
            fields = select((work / "Packages.gz").read_bytes(), package)
            destination = output / f"{browser}.deb"
            if not destination.exists() or checksum(destination) != fields["SHA256"]:
                fetch(urllib.parse.urljoin(base, fields["Filename"]), work / "package.deb")
                if checksum(work / "package.deb") != fields["SHA256"]:
                    raise ValueError(f"checksum mismatch: {browser}")
                actual = subprocess.check_output(["dpkg-deb", "-f", str(work / "package.deb"), "Package"]).decode().strip()
                if actual != package:
                    raise ValueError(f"unexpected DEB: {actual}")
                (work / "package.deb").replace(destination)
            (output / f"{browser}.sha256").write_text(fields["SHA256"] + "\n")
            (output / f"{browser}.version").write_text(fields["Version"] + "\n")
            (output / f"{browser}.source").write_text(urllib.parse.urljoin(base, fields["Filename"]) + "\n")
            print(f"{browser}: {fields['Version']} verified", flush=True)

if __name__ == "__main__":
    main()
