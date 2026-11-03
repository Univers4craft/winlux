#!/usr/bin/env python3
"""Cache official amd64 browser DEBs; verify publisher-index SHA256 hashes.

    prepare-browser-packages.py [--only edge,chrome,firefox] [SORTIE]

Aucun navigateur n'est fourni dans l'ISO (docs/BROWSERS.md) : les licences
d'Edge (« you will not ... share, publish, distribute, or lend the Software »,
section 2.8 f) et de Chrome interdisent de les redistribuer, et Winlux n'en
favorise aucun. Le navigateur choisi est téléchargé chez son éditeur sur le
PC de l'utilisateur, à sa demande : winlux-browsers --install (image) et
install-browser.sh (paquet d'intégration) appellent ce script.
"""
import gzip
import hashlib
import os
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

# Miroir de test (file:///…) : jamais utilisé par Winlux lui-même.
MIRROR = os.environ.get("WINLUX_BROWSER_MIRROR", "")


def fetch_index(url, destination):
    subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error", "--retry", "3",
                    "--connect-timeout", "15", "--max-time", "120", "--max-filesize", "67108864",
                    url, "--output", str(destination)], check=True)


def fetch_package(url, partial):
    """Paquet du navigateur (Edge 200 Mo, Chrome 120 Mo) : pas de durée maximale.

    L'ancienne limite de 5 minutes faisait échouer Edge et Chrome sur une
    connexion de moins de 5 Mbit/s (Wi-Fi d'un portable) alors que les
    petits paquets passaient. Le transfert n'est abandonné que s'il reste
    sous 1 ko/s pendant 2 minutes, et reprend là où il s'était arrêté
    (nouvel essai, ou prochain clic sur Installer)."""
    subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error",
                    "--connect-timeout", "15", "--speed-limit", "1024", "--speed-time", "120",
                    "--retry", "5", "--retry-delay", "5", "--max-filesize", "536870912",
                    "--continue-at", "-", url, "--output", str(partial)], check=True)

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
    args = sys.argv[1:]
    only = list(SOURCES)
    if args[:1] == ["--only"] and len(args) >= 2:
        only = [name for name in args[1].split(",") if name]
        args = args[2:]
    unknown = [name for name in only if name not in SOURCES]
    if unknown:
        raise SystemExit(f"navigateur inconnu : {', '.join(unknown)}")
    output = pathlib.Path(args[0] if args else "build/browser-packages").resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=output, prefix=".download-") as work:
        work = pathlib.Path(work)
        for browser in only:
            package, base, index = SOURCES[browser]
            base = MIRROR or base
            fetch_index(base + index, work / "Packages.gz")
            fields = select((work / "Packages.gz").read_bytes(), package)
            destination = output / f"{browser}.deb"
            if not destination.exists() or checksum(destination) != fields["SHA256"]:
                # Téléchargement partiel gardé d'un essai à l'autre, propre à la version.
                partial = output / f".{browser}-{fields['SHA256'][:16]}.partial"
                for old in output.glob(f".{browser}-*.partial"):
                    if old != partial:
                        old.unlink()
                fetch_package(urllib.parse.urljoin(base, fields["Filename"]), partial)
                if checksum(partial) != fields["SHA256"]:
                    partial.unlink()
                    raise ValueError(f"checksum mismatch: {browser}")
                actual = subprocess.check_output(["dpkg-deb", "-f", str(partial), "Package"]).decode().strip()
                if actual != package:
                    partial.unlink()
                    raise ValueError(f"unexpected DEB: {actual}")
                partial.replace(destination)
            (output / f"{browser}.sha256").write_text(fields["SHA256"] + "\n")
            (output / f"{browser}.version").write_text(fields["Version"] + "\n")
            (output / f"{browser}.source").write_text(urllib.parse.urljoin(base, fields["Filename"]) + "\n")
            print(f"{browser}: {fields['Version']} verified", flush=True)

if __name__ == "__main__":
    main()
