"""Image ISO de Winlux : contrôle, somme SHA-256, recherche et téléchargement."""

import hashlib
import json
import os
import re
import urllib.request
from pathlib import Path

from . import REPO, version

RELEASE_API = os.environ.get("WINLUX_USB_RELEASE_API",
                             f"https://api.github.com/repos/{REPO}/releases/latest")
ISO_ASSET = re.compile(r"^winlux-live-.+\.iso$")
CHUNK = 4 << 20


class ImageError(Exception):
    """Erreur liée à l'image ; `key` est la clé du message traduit."""

    def __init__(self, key, **fields):
        super().__init__(key)
        self.key = key
        self.fields = fields


class Cancelled(Exception):
    pass


def check_image(path):
    """Vérifie qu'un fichier est une image ISO hybride (démarrable depuis une clé).

    Renvoie sa taille. Une ISO 9660 porte « CD001 » au secteur 16 ; une ISO
    hybride (grub-mkrescue, celle de Winlux) porte en plus une table MBR,
    signature 0x55AA à l'octet 510, ce qui la rend amorçable une fois copiée
    brute sur une clé.
    """
    try:
        size = os.path.getsize(path)
        with open(path, "rb") as f:
            head = f.read(0x8006)
    except OSError as exc:
        raise ImageError("err_read_image", error=str(exc)) from exc
    if len(head) < 0x8006 or head[0x8001:0x8006] != b"CD001":
        raise ImageError("err_not_iso")
    if head[510:512] != b"\x55\xaa":
        raise ImageError("err_not_hybrid")
    return size


def sha256_file(path, progress=None, cancel=None):
    digest = hashlib.sha256()
    total = os.path.getsize(path)
    done = 0
    with open(path, "rb") as f:
        while True:
            if cancel is not None and cancel.is_set():
                raise Cancelled()
            block = f.read(CHUNK)
            if not block:
                break
            digest.update(block)
            done += len(block)
            if progress:
                progress(done, total)
    return digest.hexdigest()


def parse_sha256_text(text, name=None):
    """Première somme d'un fichier « <somme>  <nom> » (format sha256sum)."""
    for line in text.splitlines():
        parts = line.strip().split()
        if parts and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
            if name is None or len(parts) < 2 or Path(parts[-1].lstrip("*")).name == name:
                return parts[0].lower()
    return None


def sidecar_sha256(path):
    """Somme publiée à côté d'une ISO locale (fichier .sha256), si présente."""
    side = Path(str(path) + ".sha256")
    try:
        return parse_sha256_text(side.read_text(encoding="utf-8", errors="replace"), Path(path).name)
    except OSError:
        return None


def _open(url, timeout=30, headers=None):
    req = urllib.request.Request(url, headers={"User-Agent": f"winlux-usb-creator/{version()}",
                                               **(headers or {})})
    return urllib.request.urlopen(req, timeout=timeout)


def latest_release(timeout=20):
    """Dernière version publiée : {tag, name, size, url, sha256}."""
    try:
        with _open(RELEASE_API, timeout, {"Accept": "application/vnd.github+json"}) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except (OSError, ValueError) as exc:
        raise ImageError("err_release", error=str(exc)) from exc
    assets = data.get("assets") or []
    iso = next((a for a in assets if ISO_ASSET.match(a.get("name", ""))), None)
    if iso is None:
        raise ImageError("err_release", error="no ISO")
    sha = None
    digest = iso.get("digest") or ""
    if digest.startswith("sha256:"):
        sha = digest.split(":", 1)[1].lower()
    if sha is None:
        side = next((a for a in assets if a.get("name") == iso["name"] + ".sha256"), None)
        if side:
            try:
                with _open(side["browser_download_url"], timeout) as resp:
                    sha = parse_sha256_text(resp.read(4096).decode("utf-8", "replace"), iso["name"])
            except OSError:
                sha = None
    return {"tag": data.get("tag_name") or "", "name": iso["name"], "size": int(iso.get("size") or 0),
            "url": iso["browser_download_url"], "sha256": sha}


def download_dir():
    """Dossier Téléchargements de l'utilisateur (Windows, XDG, sinon ~/Downloads)."""
    home = Path.home()
    if os.name != "nt":
        conf = home / ".config/user-dirs.dirs"
        try:
            for line in conf.read_text(encoding="utf-8").splitlines():
                m = re.match(r'\s*XDG_DOWNLOAD_DIR="(.*)"', line)
                if m:
                    return Path(m.group(1).replace("$HOME", str(home)))
        except OSError:
            pass
        for name in ("Téléchargements", "Downloads"):
            if (home / name).is_dir():
                return home / name
    return home / "Downloads"


def download(release, dest_dir=None, progress=None, cancel=None, checksum_progress=None):
    """Télécharge l'ISO (reprise possible) et vérifie sa somme. Renvoie le chemin."""
    dest_dir = Path(dest_dir or download_dir())
    dest_dir.mkdir(parents=True, exist_ok=True)
    final = dest_dir / release["name"]
    total = release["size"]
    expected = release.get("sha256")
    if final.is_file() and (not total or final.stat().st_size == total):
        # Déjà téléchargée : on la garde si elle est intacte.
        if not expected or sha256_file(final, checksum_progress, cancel) == expected:
            return final
        final.unlink()
    part = dest_dir / (release["name"] + ".part")
    have = part.stat().st_size if part.is_file() else 0
    if total and have > total:
        part.unlink()
        have = 0
    headers = {"Range": f"bytes={have}-"} if have else {}
    try:
        resp = _open(release["url"], 60, headers)
    except OSError as exc:
        raise ImageError("err_download", error=str(exc)) from exc
    with resp:
        if have and getattr(resp, "status", 200) != 206:
            have = 0  # le serveur ignore la reprise : on repart de zéro
        if not total:
            total = have + int(resp.headers.get("Content-Length") or 0)
        try:
            with open(part, "ab" if have else "wb") as out:
                done = have
                while True:
                    if cancel is not None and cancel.is_set():
                        raise Cancelled()
                    block = resp.read(1 << 20)
                    if not block:
                        break
                    out.write(block)
                    done += len(block)
                    if progress:
                        progress(done, total)
        except OSError as exc:
            raise ImageError("err_download", error=str(exc)) from exc
    if total and part.stat().st_size != total:
        raise ImageError("err_download", error=f"{part.stat().st_size} / {total}")
    if expected and sha256_file(part, checksum_progress, cancel) != expected:
        part.unlink()
        raise ImageError("err_checksum")
    os.replace(part, final)
    return final
