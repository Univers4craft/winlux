#!/usr/bin/env python3
"""Cache official amd64 browser DEBs; verify publisher-index SHA256 hashes.

    prepare-browser-packages.py [--only edge,chrome,firefox] [SORTIE]
    prepare-browser-packages.py --info edge|chrome|firefox

--info lit seulement l'index de l'éditeur, sans rien télécharger d'autre, et
écrit version=, source=, sha256= et size= : Paramètres les montre avant
l'installation, pour qu'on la confirme ou la refuse (#923).

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

# Langues de Winlux autres que l'anglais : paquets de langue officiels de
# Firefox (firefox-l10n-<langue>, même dépôt Mozilla), installés avec lui pour
# qu'il s'affiche dans la langue du système. Chrome et Edge les contiennent déjà.
# Ils sont « Architecture: all » : le dépôt les liste dans binary-all/Packages,
# pas dans l'index binary-amd64 du navigateur (v0.2.19 : aucun trouvé).
LANGPACKS = {"firefox": ("firefox-l10n-{lang}", ("fr", "de", "es"), "dists/mozilla/main/binary-all/Packages")}

# Mozilla n'a pas de « firefox-l10n-es » : l'espagnol existe par pays
# (es-es, es-ar, es-cl, es-mx). Le pays vient de la langue du système
# (es_AR.UTF-8 → es-ar), l'Espagne sinon (#432).
SPANISH_REGIONS = {"es": "es-es", "ar": "es-ar", "cl": "es-cl", "mx": "es-mx"}


def langpack_names(lang):
    """Noms possibles du paquet de langue, du plus adapté au repli."""
    if lang != "es":
        return [lang]
    locale = os.environ.get("LC_ALL") or os.environ.get("LC_MESSAGES") or os.environ.get("LANG") or ""
    region = locale.split(".")[0].partition("_")[2].lower()
    names = [SPANISH_REGIONS[region]] if locale.lower().startswith("es") and region in SPANISH_REGIONS else []
    return names + [n for n in ("es-es", "es") if n not in names]


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

def select(data, package, architectures=("amd64",)):
    if data.startswith(b"\x1f\x8b"):
        data = gzip.decompress(data)
    best = None
    for stanza in data.decode("utf-8").split("\n\n"):
        fields = dict(line.split(": ", 1) for line in stanza.splitlines() if ": " in line and not line.startswith(" "))
        if fields.get("Package") != package or fields.get("Architecture") not in architectures:
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

def info(browser):
    """Version, adresse exacte, empreinte et taille annoncées par l'index (#923)."""
    if browser not in SOURCES:
        raise SystemExit(f"navigateur inconnu : {browser}")
    package, base, index = SOURCES[browser]
    base = MIRROR or base
    with tempfile.TemporaryDirectory(prefix="winlux-browser-info-") as work:
        path = pathlib.Path(work) / "Packages"
        fetch_index(base + index, path)
        fields = select(path.read_bytes(), package)
    size = fields.get("Size", "")
    print(f"version={fields['Version']}")
    print(f"source={urllib.parse.urljoin(base, fields['Filename'])}")
    print(f"sha256={fields['SHA256']}")
    print(f"size={size if size.isdigit() else ''}")


def main():
    args = sys.argv[1:]
    if args[:1] == ["--info"] and len(args) == 2:
        info(args[1])
        return
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
            fetch_langpacks(browser, base, work, output)


def fetch_langpacks(browser, base, work, output):
    """Paquets de langue : facultatifs, un échec n'empêche pas l'installation."""
    if browser not in LANGPACKS:
        return
    template, langs, index_path = LANGPACKS[browser]
    index = work / "Packages-all"
    try:
        fetch_index(base + index_path, index)
        data = index.read_bytes()
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"{browser}-l10n: index des paquets de langue indisponible ({error})", file=sys.stderr, flush=True)
        return
    for lang in langs:
        name = f"{browser}-l10n-{lang}"
        try:
            fields = None
            for variant in langpack_names(lang):
                try:
                    fields = select(data, template.format(lang=variant), ("all", "amd64"))
                    break
                except ValueError as missing:
                    if "package not found" not in str(missing):
                        raise
            if fields is None:
                raise ValueError(f"package not found: {template.format(lang=lang)}")
            destination = output / f"{name}.deb"
            if not destination.exists() or checksum(destination) != fields["SHA256"]:
                partial = output / f".l10n-{name}.partial"
                partial.unlink(missing_ok=True)
                fetch_package(urllib.parse.urljoin(base, fields["Filename"]), partial)
                if checksum(partial) != fields["SHA256"]:
                    partial.unlink()
                    raise ValueError("checksum mismatch")
                partial.replace(destination)
            (output / f"{name}.sha256").write_text(fields["SHA256"] + "\n")
            # Nom réel du paquet (firefox-l10n-es-es) : winlux-browsers le contrôle.
            (output / f"{name}.package").write_text(fields["Package"] + "\n")
            print(f"{name}: {fields['Package']} {fields['Version']} verified", flush=True)
        except (OSError, ValueError, subprocess.CalledProcessError) as error:
            for stale in (output / f"{name}.deb", output / f"{name}.sha256", output / f"{name}.package"):
                stale.unlink(missing_ok=True)
            print(f"{name}: indisponible ({error})", file=sys.stderr, flush=True)

if __name__ == "__main__":
    main()
