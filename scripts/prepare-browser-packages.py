#!/usr/bin/env python3
"""Cache official amd64 browser DEBs; verify publisher-index SHA256 hashes.

    prepare-browser-packages.py [--only brave,chrome,edge,firefox,opera,operagx,tor,vivaldi] [SORTIE]
    prepare-browser-packages.py --info ID

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
import base64
import gzip
import hashlib
import json
import lzma
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import urllib.parse

# Dépôts APT officiels des éditeurs (id : paquet, dépôt, index). Un index
# peut être donné sous plusieurs formes (Packages.gz, Packages…) : la première
# qui répond est lue. Un paquet peut avoir plusieurs noms acceptés (le premier
# trouvé dans l'index est pris ; winlux-browsers contrôle le nom réel). Relevé
# des versions Linux officielles : docs/BROWSERS.md (#1716).
SOURCES = {
    "brave": (("brave-browser",), "https://brave-browser-apt-release.s3.brave.com/",
              ("dists/stable/main/binary-amd64/Packages.gz", "dists/stable/main/binary-amd64/Packages")),
    "chrome": ("google-chrome-stable", "https://dl.google.com/linux/chrome/deb/", "dists/stable/main/binary-amd64/Packages.gz"),
    "edge": ("microsoft-edge-stable", "https://packages.microsoft.com/repos/edge/", "dists/stable/main/binary-amd64/Packages.gz"),
    "firefox": ("firefox", "https://packages.mozilla.org/apt/", "dists/mozilla/main/binary-amd64/Packages"),
    "opera": (("opera-stable",), "https://deb.opera.com/opera-stable/",
              ("dists/stable/non-free/binary-amd64/Packages.gz", "dists/stable/non-free/binary-amd64/Packages")),
    # Opera GX existe sur Linux depuis le 19 mars 2026 (.deb d'opera.com/gx/linux,
    # qui configure ce dépôt pour ses mises à jour).
    "operagx": (("opera-gx-stable", "opera-gx"), "https://deb.opera.com/opera-gx-stable/",
                ("dists/stable/non-free/binary-amd64/Packages.gz", "dists/stable/non-free/binary-amd64/Packages")),
    "vivaldi": (("vivaldi-stable",), "https://repo.vivaldi.com/archive/deb/",
                ("dists/stable/main/binary-amd64/Packages.gz", "dists/stable/main/binary-amd64/Packages")),
}

# Tor Browser n'a ni .deb ni dépôt APT : archive officielle tor-browser-linux-
# x86_64-<version>.tar.xz. La version vient du fichier de téléchargement du
# projet Tor ; l'archive est acceptée seulement si (1) la liste
# sha256sums-signed-build.txt est signée par la clé des développeurs de Tor
# Browser, (2) son SHA-256 est celui de cette liste et (3) sa propre signature
# .asc est valide avec la même clé. La clé est lue chez le projet Tor (WKD de
# torproject.org, puis keys.openpgp.org, le serveur que cite Tor) et refusée
# si son empreinte n'est pas celle épinglée ici.
TOR_ID = "tor"
TOR_PACKAGE = "tor-browser"
TOR_DOWNLOADS = "https://aus1.torproject.org/torbrowser/update_3/release/download-linux-x86_64.json"
TOR_FINGERPRINT = "EF6E286DDA85EA2A4BA7DE684E2C6E8793298290"
TOR_KEY_URLS = (
    "https://openpgpkey.torproject.org/.well-known/openpgpkey/torproject.org/hu/kounek7zrdx745qydx6p59t9mqjpuhdf?l=torbrowser",
    "https://keys.openpgp.org/vks/v1/by-fingerprint/" + TOR_FINGERPRINT,
)
# Hôte montré dans la confirmation (Paramètres) ; archive.torproject.org garde les anciennes versions.
TOR_HOST = "dist.torproject.org"
TOR_BINARY = re.compile(r"https://(dist|archive)\.torproject\.org/torbrowser/([0-9][0-9a-z.]*)/tor-browser-linux-x86_64-\2\.tar\.xz")
BROWSERS = sorted([*SOURCES, TOR_ID])

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

def fetch_any_index(base, paths, destination):
    """Première forme de l'index qui répond (Packages.gz, Packages…)."""
    paths = (paths,) if isinstance(paths, str) else paths
    for number, path in enumerate(paths):
        try:
            fetch_index(base + path, destination)
            return
        except subprocess.CalledProcessError:
            if number == len(paths) - 1:
                raise


def select(data, package, architectures=("amd64",)):
    if data.startswith(b"\x1f\x8b"):
        data = gzip.decompress(data)
    elif data.startswith(b"\xfd7zXZ"):
        data = lzma.decompress(data)
    names = (package,) if isinstance(package, str) else tuple(package)
    best = None
    for stanza in data.decode("utf-8").split("\n\n"):
        fields = dict(line.split(": ", 1) for line in stanza.splitlines() if ": " in line and not line.startswith(" "))
        if fields.get("Package") not in names or fields.get("Architecture") not in architectures:
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
        raise ValueError(f"package not found: {'|'.join(names)}")
    return best

def checksum(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def info(browser):
    """Version, adresse exacte, empreinte et taille annoncées par l'index (#923)."""
    if browser == TOR_ID:
        with tempfile.TemporaryDirectory(prefix="winlux-browser-info-") as work:
            release = tor_release(pathlib.Path(work))
        print(f"version={release['version']}")
        print(f"source={release['binary']}")
        print(f"sha256={release['sha256']}")
        print("size=")
        print("signature=tor")
        return
    if browser not in SOURCES:
        raise SystemExit(f"navigateur inconnu : {browser}")
    package, base, index = SOURCES[browser]
    base = MIRROR or base
    with tempfile.TemporaryDirectory(prefix="winlux-browser-info-") as work:
        path = pathlib.Path(work) / "Packages"
        fetch_any_index(base, index, path)
        fields = select(path.read_bytes(), package)
    size = fields.get("Size", "")
    print(f"version={fields['Version']}")
    print(f"source={urllib.parse.urljoin(base, fields['Filename'])}")
    print(f"sha256={fields['SHA256']}")
    print(f"size={size if size.isdigit() else ''}")


# --- Tor Browser : archive signée --------------------------------------------

def tor_settings():
    """Adresses et empreinte ; un miroir de test (file://) peut les remplacer."""
    if MIRROR.startswith("file://"):
        return (MIRROR + "download-linux-x86_64.json", os.environ.get("WINLUX_BROWSER_TOR_FINGERPRINT", TOR_FINGERPRINT),
                (MIRROR + "torbrowser.key",), re.compile(re.escape(MIRROR) + r"torbrowser/([0-9][0-9a-z.]*)/tor-browser-linux-x86_64-\1\.tar\.xz"))
    return TOR_DOWNLOADS, TOR_FINGERPRINT, TOR_KEY_URLS, TOR_BINARY


def dearmor(data):
    """Clé OpenPGP en clair (ASCII armor) → paquets binaires lus par gpgv."""
    text = data.decode("ascii", "replace")
    if "-----BEGIN PGP PUBLIC KEY BLOCK-----" not in text:
        return data
    body = text.split("-----BEGIN PGP PUBLIC KEY BLOCK-----", 1)[1].split("-----END PGP PUBLIC KEY BLOCK-----", 1)[0]
    lines = [line.strip() for line in body.strip().splitlines()]
    # En-têtes facultatifs (« Comment: … ») puis une ligne vide ; « =XXXX » : somme de contrôle.
    if "" in lines:
        lines = lines[lines.index("") + 1:]
    return base64.b64decode("".join(line for line in lines if line and not line.startswith("=")))


def primary_fingerprint(data):
    """Empreinte V4 du premier paquet « clé publique » (RFC 4880 §12.2), sans gpg."""
    if len(data) < 2 or not data[0] & 0x80:
        return ""
    if data[0] & 0x40:  # en-tête au nouveau format
        tag, first = data[0] & 0x3F, data[1]
        if first < 192:
            length, start = first, 2
        elif first < 224:
            length, start = ((first - 192) << 8) + data[2] + 192, 3
        elif first == 255:
            length, start = int.from_bytes(data[2:6], "big"), 6
        else:
            return ""
    else:  # ancien format
        tag, kind = (data[0] >> 2) & 0x0F, data[0] & 0x03
        size = {0: 1, 1: 2, 2: 4}.get(kind)
        if size is None:
            return ""
        length, start = int.from_bytes(data[1:1 + size], "big"), 1 + size
    body = data[start:start + length]
    if tag != 6 or len(body) != length or not body or body[0] != 4:
        return ""
    return hashlib.sha1(b"\x99" + length.to_bytes(2, "big") + body).hexdigest().upper()


def gpgv_ok(keyring, signature, data, fingerprint):
    """Signature valide faite par la clé dont l'empreinte PRINCIPALE est épinglée."""
    home = keyring.parent / "gnupg"
    home.mkdir(mode=0o700, exist_ok=True)
    result = subprocess.run(["gpgv", "--homedir", str(home), "--keyring", str(keyring), "--status-fd", "1",
                             str(signature), str(data)], capture_output=True, text=True)
    good = valid = False
    for line in result.stdout.splitlines():
        parts = line.split()
        if parts[:2] == ["[GNUPG:]", "GOODSIG"]:
            good = True
        # VALIDSIG <empreinte de la sous-clé> … <empreinte de la clé principale>
        if parts[:2] == ["[GNUPG:]", "VALIDSIG"] and parts[-1].upper() == fingerprint.upper():
            valid = True
    return result.returncode == 0 and good and valid


def tor_key(work, fingerprint, urls):
    """Clé des développeurs de Tor Browser, lue chez Tor ; empreinte contrôlée."""
    keyring = work / "torbrowser.gpg"
    for url in urls:
        try:
            fetch_index(url, work / "torbrowser.key")
            keyring.write_bytes(dearmor((work / "torbrowser.key").read_bytes()))
        except (OSError, ValueError, subprocess.CalledProcessError):
            continue
        # Première clé du fichier = clé principale : elle doit être celle épinglée.
        if primary_fingerprint(keyring.read_bytes()) == fingerprint.upper():
            return keyring
    raise ValueError("clé des développeurs de Tor Browser introuvable ou empreinte inattendue")


def tor_release(work):
    """Version, adresse de l'archive et SHA-256 tiré de la liste signée."""
    downloads, fingerprint, key_urls, pattern = tor_settings()
    fetch_index(downloads, work / "download.json")
    data = json.loads((work / "download.json").read_text())
    binary, signature, version = data.get("binary", ""), data.get("sig", ""), str(data.get("version", ""))
    match = pattern.fullmatch(binary)
    if not match or match.groups()[-1] != version or signature != binary + ".asc":
        raise ValueError(f"adresse de Tor Browser inattendue : {binary}")
    folder = binary.rsplit("/", 1)[0] + "/"
    keyring = tor_key(work, fingerprint, key_urls)
    fetch_index(folder + "sha256sums-signed-build.txt", work / "sums.txt")
    fetch_index(folder + "sha256sums-signed-build.txt.asc", work / "sums.txt.asc")
    if not gpgv_ok(keyring, work / "sums.txt.asc", work / "sums.txt", fingerprint):
        raise ValueError("signature de la liste SHA-256 de Tor Browser invalide")
    name = binary.rsplit("/", 1)[1]
    sums = [line.split() for line in (work / "sums.txt").read_text().splitlines()]
    digest = next((parts[0] for parts in sums if len(parts) == 2 and parts[1].lstrip("*") == name), "")
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise ValueError(f"{name} absent de la liste SHA-256 signée")
    return {"version": version, "binary": binary, "sig": signature, "sha256": digest, "keyring": keyring,
            "fingerprint": fingerprint}


def fetch_tor(output, work):
    release = tor_release(work)
    destination = output / f"{TOR_ID}.tar.xz"
    if not destination.exists() or checksum(destination) != release["sha256"]:
        partial = output / f".{TOR_ID}-{release['sha256'][:16]}.partial"
        for old in output.glob(f".{TOR_ID}-*.partial"):
            if old != partial:
                old.unlink()
        fetch_package(release["binary"], partial)
        if checksum(partial) != release["sha256"]:
            partial.unlink()
            raise ValueError("checksum mismatch: tor")
        fetch_index(release["sig"], work / "tor.tar.xz.asc")
        if not gpgv_ok(release["keyring"], work / "tor.tar.xz.asc", partial, release["fingerprint"]):
            partial.unlink()
            raise ValueError("signature de Tor Browser invalide")
        partial.replace(destination)
    else:
        fetch_index(release["sig"], work / "tor.tar.xz.asc")
    # Signature et clé gardées : winlux-browsers revérifie l'archive avant de l'extraire.
    (output / f"{TOR_ID}.tar.xz.asc").write_bytes((work / "tor.tar.xz.asc").read_bytes())
    (output / f"{TOR_ID}.keyring").write_bytes(release["keyring"].read_bytes())
    (output / f"{TOR_ID}.fingerprint").write_text(release["fingerprint"] + "\n")
    (output / f"{TOR_ID}.sha256").write_text(release["sha256"] + "\n")
    (output / f"{TOR_ID}.version").write_text(release["version"] + "\n")
    (output / f"{TOR_ID}.source").write_text(release["binary"] + "\n")
    (output / f"{TOR_ID}.package").write_text(TOR_PACKAGE + "\n")
    print(f"{TOR_ID}: {release['version']} verified (SHA-256 + signature {release['fingerprint']})", flush=True)


def main():
    args = sys.argv[1:]
    if args[:1] == ["--info"] and len(args) == 2:
        info(args[1])
        return
    only = list(BROWSERS)
    if args[:1] == ["--only"] and len(args) >= 2:
        only = [name for name in args[1].split(",") if name]
        args = args[2:]
    unknown = [name for name in only if name not in BROWSERS]
    if unknown:
        raise SystemExit(f"navigateur inconnu : {', '.join(unknown)}")
    output = pathlib.Path(args[0] if args else "build/browser-packages").resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=output, prefix=".download-") as work:
        work = pathlib.Path(work)
        for browser in only:
            if browser == TOR_ID:
                fetch_tor(output, work)
                continue
            package, base, index = SOURCES[browser]
            base = MIRROR or base
            fetch_any_index(base, index, work / "Packages.gz")
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
                if actual != fields["Package"]:
                    partial.unlink()
                    raise ValueError(f"unexpected DEB: {actual}")
                partial.replace(destination)
            (output / f"{browser}.sha256").write_text(fields["SHA256"] + "\n")
            (output / f"{browser}.version").write_text(fields["Version"] + "\n")
            (output / f"{browser}.source").write_text(urllib.parse.urljoin(base, fields["Filename"]) + "\n")
            (output / f"{browser}.package").write_text(fields["Package"] + "\n")
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
