#!/usr/bin/python3
"""Store Winlux : une seule bibliothèque pour les logiciels Linux et Windows.

Sources (chacune n'apparaît que si elle est utilisable sur la machine) :
  apt       paquets Debian/Ubuntu, métadonnées AppStream (DEP-11) + sélection Winlux
  flatpak   dépôts Flatpak configurés (Flathub...), métadonnées AppStream XML
  snap      Snap Store, via l'API locale de snapd
  appimage  catalogue AppImageHub, AppImage installées dans ~/Applications
  windows   catalogues de logiciels Windows interchangeables (voir docs/STORE.md),
            exécutés par le runtime Winlux

Commandes (écrites par le shell via ShellExecute, résultats dans $run) :
  refresh [--force]   index de la bibliothèque     -> store.txt
  details <clé>       fiche complète d'un logiciel -> store-detail.txt
  install|remove|update <clé>, update-all, install-file <fichier>
                      avancement                   -> store-progress.txt
  starter --list | --resume | <id>...
                      logiciels courants de l'appli Bienvenue, installés
                      l'un après l'autre avec reprise -> starter.txt
Une clé est « source:référence », par exemple apt:vlc ou flatpak:org.gimp.GIMP.

Le Store n'utilise que les outils standards (apt-get, flatpak, snap) avec
leurs verrous : apt update / apt upgrade en terminal restent inchangés.
"""
import gzip
import hashlib
import html
import io
import http.client
import json
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess
import sys
import tarfile
import threading
import time
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

ROOT = os.environ.get("WINLUX_CONTROL_ROOT", "")
RUN = Path(ROOT + "/run/winlux")
HOME = Path(os.environ.get("HOME", "/root"))
CACHE = Path(os.environ.get("XDG_CACHE_HOME", HOME / ".cache")) / "winlux/store"
DATA = Path(os.environ.get("XDG_DATA_HOME", HOME / ".local/share"))
STATE = DATA / "winlux/store-state.json"
CONFIG_DIRS = [Path(ROOT + "/etc/winlux/store-sources.d"),
               Path(os.environ.get("XDG_CONFIG_HOME", HOME / ".config")) / "winlux/store-sources.d"]
CATALOG = Path(os.environ.get("WINLUX_STORE_CATALOG", ROOT + "/usr/share/winlux/store-catalog.txt"))
def interface_lang():
    """Langue de l'interface : WINLUX_LANG, sinon /etc/winlux/locale.conf
    (Paramètres > Langue), sinon LANG ; fr, en, es ou de."""
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path(ROOT + "/etc/winlux/locale.conf").read_text().splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


LANG = interface_lang()

# Messages relus et affichés par le Store (store-progress.txt).
T = {
    "fr": {"busy": "Une installation est déjà en cours.", "preparing": "Préparation...",
           "all_software": "Tous les logiciels", "unknown": "Logiciel inconnu : {target}",
           "installed": "{name} est installé.", "removed": "{name} est désinstallé.",
           "updated": "{name} est à jour.", "all_updated": "{name} sont à jour.",
           "failed": "Échec ({code}) : voir {log}", "not_found": "Fichier introuvable : {path}",
           "installing": "Installation de {name}...", "install_failed": "Installation impossible : {error}",
           "offline": "Téléchargement impossible : vérifiez la connexion réseau.",
           "win32": "Programme Windows 32 bits : pas encore pris en charge par le runtime Winlux.",
           "admin": "Droits administrateur requis pour installer des logiciels.",
           "sha": "Empreinte SHA-256 invalide : installateur refusé.",
           "latest": "Recherche de la dernière version...", "download": "Téléchargement de {name}...",
           "runtime": "Installation de {name} avec le runtime Winlux...",
           "installed_version": "installé"},
    "en": {"busy": "An installation is already running.", "preparing": "Preparing...",
           "all_software": "All software", "unknown": "Unknown software: {target}",
           "installed": "{name} is installed.", "removed": "{name} is uninstalled.",
           "updated": "{name} is up to date.", "all_updated": "{name} is up to date.",
           "failed": "Failed ({code}): see {log}", "not_found": "File not found: {path}",
           "installing": "Installing {name}...", "install_failed": "Installation failed: {error}",
           "offline": "Download failed: check the network connection.",
           "win32": "32-bit Windows program: not supported by the Winlux runtime yet.",
           "admin": "Administrator rights are required to install software.",
           "sha": "Invalid SHA-256 fingerprint: installer rejected.",
           "latest": "Looking for the latest version...", "download": "Downloading {name}...",
           "runtime": "Installing {name} with the Winlux runtime...",
           "installed_version": "installed"},
    "es": {"busy": "Ya hay una instalación en curso.", "preparing": "Preparando...",
           "all_software": "Todo el software", "unknown": "Software desconocido: {target}",
           "installed": "{name} está instalado.", "removed": "{name} está desinstalado.",
           "updated": "{name} está actualizado.", "all_updated": "{name} está actualizado.",
           "failed": "Error ({code}): consulte {log}", "not_found": "Archivo no encontrado: {path}",
           "installing": "Instalando {name}...", "install_failed": "No se pudo instalar: {error}",
           "offline": "No se pudo descargar: compruebe la conexión de red.",
           "win32": "Programa de Windows de 32 bits: el runtime de Winlux aún no lo admite.",
           "admin": "Se necesitan derechos de administrador para instalar software.",
           "sha": "Huella SHA-256 no válida: instalador rechazado.",
           "latest": "Buscando la última versión...", "download": "Descargando {name}...",
           "runtime": "Instalando {name} con el runtime de Winlux...",
           "installed_version": "instalado"},
    "de": {"busy": "Es läuft bereits eine Installation.", "preparing": "Vorbereitung...",
           "all_software": "Alle Programme", "unknown": "Unbekannte Software: {target}",
           "installed": "{name} ist installiert.", "removed": "{name} ist deinstalliert.",
           "updated": "{name} ist auf dem neuesten Stand.",
           "all_updated": "{name} sind auf dem neuesten Stand.",
           "failed": "Fehlgeschlagen ({code}): siehe {log}", "not_found": "Datei nicht gefunden: {path}",
           "installing": "{name} wird installiert...", "install_failed": "Installation fehlgeschlagen: {error}",
           "offline": "Download fehlgeschlagen: Überprüfen Sie die Netzwerkverbindung.",
           "win32": "32-Bit-Windows-Programm: von der Winlux-Laufzeit noch nicht unterstützt.",
           "admin": "Zum Installieren von Software sind Administratorrechte erforderlich.",
           "sha": "Ungültiger SHA-256-Fingerabdruck: Installationsprogramm abgelehnt.",
           "latest": "Neueste Version wird gesucht...", "download": "{name} wird heruntergeladen...",
           "runtime": "{name} wird mit der Winlux-Laufzeit installiert...",
           "installed_version": "installiert"},
}


def tr(key, **values):
    text = T[LANG].get(key) or T["fr"][key]
    return text.format(**values) if values else text

OFFLINE = os.environ.get("WINLUX_STORE_OFFLINE") == "1"
REMOTE_TTL = 6 * 3600
MAX_INDEX = 2500
CATEGORIES = ["navigateurs", "bureautique", "multimedia", "developpement", "jeux", "utilitaires"]

# ---------------------------------------------------------------------------
# Outils communs
# ---------------------------------------------------------------------------

def have(cmd):
    return shutil.which(cmd) is not None

def run(cmd, timeout=60, env=None):
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=env)
        return out.returncode, out.stdout
    except (OSError, subprocess.SubprocessError):
        return 127, ""

def atomic_write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f"{path.name}.{os.getpid()}")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)

def field(value, limit=400):
    """Valeur sûre pour une ligne « clé=a|b|c » relue par le shell."""
    value = re.sub(r"\s+", " ", "" if value is None else str(value)).replace("|", "/").strip()
    return value[:limit]

def is_root():
    return os.geteuid() == 0

def admin(cmd):
    if is_root():
        return cmd
    if have("sudo") and run(["sudo", "-n", "true"], 5)[0] == 0:
        return ["sudo", "-n", *cmd]
    # Bureau sous le compte de l'utilisateur : relais root de Winlux, avec le
    # Contrôle de compte d'utilisateur pour installer ou désinstaller.
    if have("winlux-root") and os.path.exists(os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")):
        return ["winlux-root", "exec", *cmd]
    if have("pkexec"):
        return ["pkexec", *cmd]
    return None

def load_json(path, default):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return default

def save_state(state):
    atomic_write(STATE, json.dumps(state, indent=1, sort_keys=True))

def fetch(url, timeout=15, limit=64 << 20):
    """Télécharge une URL http(s)/file ; None si hors ligne ou en erreur."""
    if OFFLINE and not url.startswith("file:"):
        return None
    if not url.startswith(("https://", "http://", "file:")):
        return None
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "Winlux-Store/1.0"})
        deadline = time.monotonic() + timeout * 4
        chunks, size = [], 0
        with urllib.request.urlopen(req, timeout=timeout) as r:
            while size <= limit:
                if time.monotonic() > deadline:
                    return None  # serveur trop lent : on réessaiera plus tard
                chunk = r.read(1 << 16)
                if not chunk:
                    break
                chunks.append(chunk)
                size += len(chunk)
        return b"".join(chunks) if size <= limit else None
    except Exception:
        return None

def cached_fetch(url, name, ttl=REMOTE_TTL, force=False):
    """Copie locale d'un catalogue distant, rafraîchie après `ttl` secondes."""
    path = CACHE / "remote" / name
    fresh = path.exists() and time.time() - path.stat().st_mtime < ttl
    if fresh and not force:
        return path.read_bytes()
    data = fetch(url)
    if data is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return data
    return path.read_bytes() if path.exists() else None

def media(url, kind="img"):
    """Image locale (PNG) pour une URL ; téléchargée et mise en cache."""
    if not url:
        return ""
    if url.startswith("/"):
        return url if url.lower().endswith(".png") and os.path.exists(url) else ""
    digest = hashlib.sha256(url.encode()).hexdigest()[:24]
    path = CACHE / kind / f"{digest}.png"
    if path.exists():
        return str(path)
    data = fetch(url, timeout=20, limit=16 << 20)
    if not data:
        return ""
    if not data.startswith(b"\x89PNG"):
        data = to_png(data)
        if not data:
            return ""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return str(path)

def to_png(data):
    """Convertit une image (JPEG...) en PNG avec un outil local s'il existe."""
    for cmd in (["ffmpeg", "-loglevel", "error", "-i", "pipe:0", "-f", "image2", "-c:v", "png", "pipe:1"],
                ["convert", "-", "png:-"]):
        if have(cmd[0]):
            try:
                out = subprocess.run(cmd, input=data, capture_output=True, timeout=30)
                if out.returncode == 0 and out.stdout.startswith(b"\x89PNG"):
                    return out.stdout
            except (OSError, subprocess.SubprocessError):
                pass
    return None

def category_of(cats):
    cats = {c.lower() for c in cats}
    for keys, name in ((("webbrowser",), "navigateurs"), (("game", "games"), "jeux"),
                       (("development", "ide", "programming"), "developpement"),
                       (("office", "wordprocessor", "spreadsheet", "presentation", "productivity"), "bureautique"),
                       (("audiovideo", "audio", "video", "graphics", "photography", "music", "player",
                         "photo-and-video", "music-and-audio", "art-and-design"), "multimedia"),
                       ):
        if cats & set(keys):
            return name
    return "utilitaires"

def html_text(text):
    """Description AppStream (<p>, <li>) -> paragraphes de texte simple."""
    text = re.sub(r"(?i)<li>", "\n• ", text or "")
    text = re.sub(r"(?i)</p>|<br\s*/?>|</ul>|</ol>", "\n", text)
    text = html.unescape(re.sub(r"<[^>]+>", "", text))
    return [p.strip() for p in re.split(r"\n+", text) if p.strip()]

def localized(value):
    if isinstance(value, dict):
        return value.get(LANG) or value.get("C") or value.get("en") or next(iter(value.values()), "")
    return value or ""

class App:
    """Fiche normalisée, commune à toutes les sources."""
    def __init__(self, source, ref, name, **kw):
        self.source, self.ref, self.name = source, ref, name
        self.summary = kw.get("summary", "")
        self.description = kw.get("description", [])
        self.category = kw.get("category", "utilitaires")
        self.publisher = kw.get("publisher", "")
        self.license = kw.get("license", "")
        self.homepage = kw.get("homepage", "")
        self.icon = kw.get("icon", "")          # chemin local ou URL
        self.screenshots = kw.get("screenshots", [])
        self.releases = kw.get("releases", [])  # (version, date, texte)
        self.version = kw.get("version", "")    # version installée
        self.available = kw.get("available", "")
        self.size_kb = kw.get("size_kb", 0)
        self.state = kw.get("state", "a")       # a disponible, i installé, u mise à jour
        self.featured = kw.get("featured", False)
        self.appstream_id = kw.get("appstream_id", "")
        self.rank = kw.get("rank", 3)  # 0 sélection, 1 choisi par Winlux, 3 autre

    @property
    def key(self):
        return f"{self.source}:{self.ref}"

# ---------------------------------------------------------------------------
# AppStream : DEP-11 (YAML) et XML
# ---------------------------------------------------------------------------

def _scalar(text):
    text = text.strip()
    if text == "{}":
        return {}
    if text.startswith("[") and text.endswith("]"):
        inner = text[1:-1].strip()
        return [_scalar(x) for x in inner.split(",")] if inner else []
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "'\"":
        text = text[1:-1]
        return text.replace("''", "'") if text and text[0] != '"' else text
    return text

def parse_yaml(text):
    """Sous-ensemble YAML utilisé par DEP-11 : documents, mappings, listes,
    scalaires et blocs « | » / « > »."""
    docs = []
    for chunk in re.split(r"^---[^\n]*\n", text, flags=re.M):
        if chunk.strip():
            lines = [l for l in chunk.split("\n") if l.strip() and not l.lstrip().startswith("#")]
            value, _ = _parse_block(lines, 0, 0)
            docs.append(value)
    return docs

def _indent(line):
    return len(line) - len(line.lstrip(" "))

def _parse_block(lines, i, indent):
    if i >= len(lines):
        return None, i
    if lines[i].lstrip().startswith("- "):
        return _parse_list(lines, i, _indent(lines[i]))
    return _parse_map(lines, i, _indent(lines[i]))

def _parse_list(lines, i, indent):
    out = []
    while i < len(lines) and _indent(lines[i]) == indent and lines[i].lstrip().startswith("- "):
        item = lines[i].lstrip()[2:]
        if re.match(r"^[\w.-]+:(\s|$)", item):
            # « - clé: valeur » : mapping dont la première clé suit le tiret.
            lines = lines[:i] + [" " * (indent + 2) + item] + lines[i + 1:]
            value, i = _parse_map(lines, i, indent + 2)
            out.append(value)
        else:
            out.append(_scalar(item))
            i += 1
    return out, i

def _parse_map(lines, i, indent):
    out = {}
    while i < len(lines) and _indent(lines[i]) == indent and not lines[i].lstrip().startswith("- "):
        m = re.match(r"^\s*([^:]+?):(?:\s+(.*))?$", lines[i])
        if not m:
            i += 1
            continue
        key, rest = m.group(1).strip().strip("'\""), (m.group(2) or "").strip()
        i += 1
        if rest in ("|", "|-", ">", ">-", "|+", ">+"):
            block = []
            while i < len(lines) and _indent(lines[i]) > indent:
                block.append(lines[i].strip())
                i += 1
            out[key] = ("\n" if rest[0] == "|" else " ").join(block)
        elif rest:
            out[key] = _scalar(rest)
        elif i < len(lines) and (_indent(lines[i]) > indent or
                                 (_indent(lines[i]) == indent and lines[i].lstrip().startswith("- "))):
            out[key], i = _parse_block(lines, i, _indent(lines[i]))
        else:
            out[key] = ""
    return out, i

def dep11_dirs():
    return [Path(ROOT + d) for d in ("/var/lib/swcatalog", "/var/lib/app-info")]

def dep11_components():
    """Composants AppStream des dépôts apt (paquet -> fiche brute)."""
    found = {}
    for base in dep11_dirs():
        for path in sorted((base / "yaml").glob("*.yml*")):
            try:
                raw = path.read_bytes()
                text = (gzip.decompress(raw) if path.suffix == ".gz" else raw).decode("utf-8", "replace")
            except (OSError, EOFError):
                continue
            docs = parse_yaml(text)
            if not docs:
                continue
            header, origin = docs[0], docs[0].get("Origin", "")
            media_base = header.get("MediaBaseUrl", "")
            for doc in docs[1:]:
                if not isinstance(doc, dict) or doc.get("Type") != "desktop-application" or not doc.get("Package"):
                    continue
                doc["_origin"], doc["_media"], doc["_base"] = origin, media_base, base
                found.setdefault(doc["Package"], doc)
    return found

def dep11_icon_archives(force=False):
    """Icônes AppStream des dépôts apt (icons-64x64.tar.gz du miroir, comme
    les télécharge appstream) quand le système ne les a pas extraites."""
    import tarfile
    import io
    for base in dep11_dirs():
        for path in sorted((base / "yaml").glob("*_dists_*_dep11_Components-*.yml*")):
            m = re.match(r"^(.+)_dists_(.+)_([^_]+)_dep11_Components-", path.name)
            if not m:
                continue
            try:
                head = gzip.open(path, "rt", errors="replace").read(4096) if path.suffix == ".gz" else path.read_text()[:4096]
            except (OSError, EOFError):
                continue
            origin = re.search(r"^Origin:\s*(\S+)", head, re.M)
            if not origin:
                continue
            dest = CACHE / "dep11-icons" / origin.group(1) / "64x64"
            if dest.is_dir() and not force:
                continue
            url = f"http://{m.group(1).replace('_', '/')}/dists/{m.group(2)}/{m.group(3)}/dep11/icons-64x64.tar.gz"
            data = fetch(url, 30, 256 << 20)
            if not data:
                continue
            # Extraction dans un dossier temporaire puis renommage : un
            # rafraîchissement interrompu ne laisse jamais un dossier partiel.
            tmp = dest.with_name(f".64x64.{os.getpid()}")
            shutil.rmtree(tmp, ignore_errors=True)
            tmp.mkdir(parents=True)
            try:
                with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
                    for member in tar:
                        name = os.path.basename(member.name)
                        if member.isfile() and name.endswith(".png") and not name.startswith("."):
                            src = tar.extractfile(member)
                            if src:
                                (tmp / name).write_bytes(src.read())
                shutil.rmtree(dest, ignore_errors=True)
                os.replace(tmp, dest)
            except (tarfile.TarError, OSError, EOFError):
                shutil.rmtree(tmp, ignore_errors=True)

def dep11_icon(doc):
    icon = doc.get("Icon") or {}
    for cached in sorted(icon.get("cached") or [], key=lambda c: -int(c.get("width", 0) or 0)):
        size = f"{cached.get('width')}x{cached.get('height')}"
        for path in (doc["_base"] / "icons" / doc["_origin"] / size / cached.get("name", ""),
                     CACHE / "dep11-icons" / doc["_origin"] / size / cached.get("name", "")):
            if path.is_file():
                return str(path)
    for remote in icon.get("remote") or []:
        url = remote.get("url", "")
        return url if url.startswith("http") else doc["_media"].rstrip("/") + "/" + url
    stock = icon.get("stock")
    return f"/usr/share/icons/hicolor/128x128/apps/{stock}.png" if stock else ""

def dep11_screens(doc):
    shots = []
    for s in doc.get("Screenshots") or []:
        if not isinstance(s, dict):
            continue
        thumbs = sorted(s.get("thumbnails") or [], key=lambda t: abs(int(t.get("width", 0) or 0) - 752))
        img = (thumbs[0] if thumbs else s.get("source-image") or {}).get("url", "")
        if img:
            shots.append(img if img.startswith("http") else doc["_media"].rstrip("/") + "/" + img)
    return shots

def dep11_releases(doc):
    out = []
    for r in doc.get("Releases") or []:
        if isinstance(r, dict) and r.get("version"):
            date = time.strftime("%Y-%m-%d", time.gmtime(int(r.get("unix-timestamp", 0) or 0))) \
                if str(r.get("unix-timestamp", "")).isdigit() else ""
            out.append((r["version"], date, " ".join(html_text(localized(r.get("description"))))))
    return out[:8]

def dep11_index():
    """Fiches AppStream apt compactes (langue de la session), mises en cache
    pour que l'ouverture d'une fiche reste instantanée."""
    path = CACHE / f"dep11-{LANG}.json"
    sources = [p for base in dep11_dirs() for p in (base / "yaml").glob("*.yml*")]
    stamp = max([p.stat().st_mtime for p in sources] + [0])
    icons = CACHE / "dep11-icons"
    if icons.is_dir():
        stamp = max([stamp] + [d.stat().st_mtime for d in icons.iterdir()])
    if path.exists() and path.stat().st_mtime >= stamp:
        cached = load_json(path, None)
        if isinstance(cached, dict):
            return cached
    out = {}
    for pkg, doc in dep11_components().items():
        out[pkg] = {
            "Name": localized(doc.get("Name")), "Summary": localized(doc.get("Summary")),
            "Categories": doc.get("Categories") or [], "DeveloperName": localized(doc.get("DeveloperName")),
            "ID": str(doc.get("ID", "")), "ProjectLicense": doc.get("ProjectLicense", ""),
            "homepage": (doc.get("Url") or {}).get("homepage", "") if isinstance(doc.get("Url"), dict) else "",
            "icon": dep11_icon(doc), "description": html_text(localized(doc.get("Description"))),
            "screenshots": dep11_screens(doc), "releases": dep11_releases(doc),
        }
    try:
        atomic_write(path, json.dumps(out))
    except OSError:
        pass
    return out

def xml_text(el, tag):
    """Texte localisé d'un élément AppStream XML (xml:lang)."""
    best = None
    for child in el.findall(tag):
        lang = child.get("{http://www.w3.org/XML/1998/namespace}lang")
        if lang == LANG:
            return "".join(child.itertext()).strip()
        if lang is None and best is None:
            best = child
    return "".join(best.itertext()).strip() if best is not None else ""

def xml_description(el):
    for d in el.findall("description"):
        lang = d.get("{http://www.w3.org/XML/1998/namespace}lang")
        if lang in (None, LANG):
            parts = []
            for child in d:
                lang2 = child.get("{http://www.w3.org/XML/1998/namespace}lang")
                if lang2 not in (None, LANG):
                    continue
                if child.tag == "p":
                    parts.append("".join(child.itertext()).strip())
                elif child.tag in ("ul", "ol"):
                    parts += ["• " + "".join(li.itertext()).strip() for li in child if
                              li.get("{http://www.w3.org/XML/1998/namespace}lang") in (None, LANG)]
            return [p for p in parts if p]
    return []

def appstream_xml(data):
    """Composants d'un fichier AppStream XML (Flathub)."""
    try:
        if data[:2] == b"\x1f\x8b":
            data = gzip.decompress(data)
        root = ET.fromstring(data)
    except (ET.ParseError, OSError, EOFError):
        return []
    out = []
    for c in root.iter("component"):
        if c.get("type") not in ("desktop-application", "desktop", None):
            continue
        cid = (c.findtext("id") or "").strip()
        bundle = (c.findtext("bundle") or "").strip()
        ref = bundle.split("/")[1] if bundle.count("/") >= 2 else cid.removesuffix(".desktop")
        if not ref:
            continue
        shots = []
        for s in c.iter("screenshot"):
            imgs = [(abs(int(i.get("width", "0") or 0) - 752), i.text.strip()) for i in s.findall("image") if i.text]
            if imgs:
                shots.append(min(imgs)[1])
        releases = []
        for r in c.iter("release"):
            ts = r.get("timestamp", "")
            date = r.get("date", "") or (time.strftime("%Y-%m-%d", time.gmtime(int(ts))) if ts.isdigit() else "")
            releases.append((r.get("version", ""), date, " ".join(xml_description(r))))
        icons = {}
        for i in c.findall("icon"):
            icons.setdefault(i.get("type"), []).append((int(i.get("height", "0") or 0), (i.text or "").strip()))
        dev = c.find("developer")
        out.append({
            "ref": ref, "id": cid, "name": xml_text(c, "name") or ref,
            "summary": xml_text(c, "summary"), "description": xml_description(c),
            "categories": [x.text or "" for x in c.iter("category")],
            "publisher": xml_text(c, "developer_name") or (xml_text(dev, "name") if dev is not None else ""),
            "license": (c.findtext("project_license") or "").strip(),
            "homepage": next((u.text.strip() for u in c.findall("url") if u.get("type") == "homepage" and u.text), ""),
            "icons": icons, "screenshots": shots[:6], "releases": releases[:8],
        })
    return out

# ---------------------------------------------------------------------------
# Sources
# ---------------------------------------------------------------------------

class Source:
    """Interface d'une source de logiciels."""
    name = ""
    label = ""

    def available(self):
        return False

    def apps(self, force=False):
        return []

    def details(self, app):
        return app

    def install(self, ref, progress):
        return 1

    def remove(self, ref, progress):
        return 1

    def update(self, refs, progress):
        return 1

    def update_all(self, progress):
        return 0


class Apt(Source):
    name, label = "apt", "Ubuntu"
    local_only = False

    def available(self):
        return have("apt-get") and have("dpkg-query")

    def curated(self):
        apps, featured = {}, set()
        try:
            lines = CATALOG.read_text(encoding="utf-8").splitlines()
        except OSError:
            lines = []
        for line in lines:
            if line.startswith("featured="):
                featured.update(line[9:].split(","))
            elif line and not line.startswith("#") and line.count("|") >= 5:
                ident, cat, name, pkg, pub, desc = line.split("|", 5)
                apps[pkg] = {"id": ident, "cat": cat, "name": name, "pub": pub, "desc": desc}
        for pkg, entry in apps.items():
            entry["featured"] = entry["id"] in featured
        return apps

    def installed(self):
        code, out = run(["dpkg-query", "-W", "-f", "${Package}\t${db:Status-Abbrev}\t${Version}\t${Installed-Size}\n"])
        result = {}
        for line in out.splitlines():
            parts = line.split("\t")
            if len(parts) == 4 and parts[1].startswith("ii"):
                result[parts[0]] = (parts[2], int(parts[3] or 0) if parts[3].isdigit() else 0)
        return result

    def upgradable(self):
        code, out = run(["apt", "list", "--upgradable"], 60, env={**os.environ, "LANG": "C"})
        result = {}
        for line in out.splitlines():
            m = re.match(r"^([^/\s]+)/\S+\s+(\S+)", line)
            if m and "upgradable" in line:
                result[m.group(1)] = m.group(2)
        return result

    def candidates(self, pkgs):
        if not pkgs or not have("apt-cache"):
            return {}
        code, out = run(["apt-cache", "policy", *pkgs], 60, env={**os.environ, "LANG": "C"})
        result, pkg = {}, None
        for line in out.splitlines():
            if line and not line.startswith(" ") and line.endswith(":"):
                pkg = line[:-1]
            elif line.strip().startswith("Candidate:") and pkg:
                cand = line.split(":", 1)[1].strip()
                if cand != "(none)":
                    result[pkg] = cand
        return result

    def refresh_lists(self):
        """apt-get update en arrière-plan si les listes ont plus de 6 h."""
        lists = Path(ROOT + "/var/lib/apt/lists")
        try:
            age = time.time() - max((p.stat().st_mtime for p in lists.glob("*_Packages*")), default=0)
        except OSError:
            age = 0
        if age > REMOTE_TTL and not OFFLINE and (is_root() or (admin(["true"]) or [""])[0] == "winlux-root"):
            run(admin(["apt-get", "-q", "-o", "DPkg::Lock::Timeout=30", "update"]) or ["true"], 600)

    def apps(self, force=False):
        if force:
            self.refresh_lists()
        if not OFFLINE and not self.local_only:
            dep11_icon_archives(force)
        curated, comps = self.curated(), dep11_index()
        installed, upgradable = self.installed(), self.upgradable()
        cands = self.candidates([p for p in curated if p not in comps])
        self.system_updates = len(upgradable)
        out = []
        for pkg in sorted(set(curated) | set(comps)):
            doc, cur = comps.get(pkg) or {}, curated.get(pkg)
            if not doc and pkg not in cands and pkg not in installed:
                continue  # absent des dépôts configurés
            ver = installed.get(pkg, ("", 0))[0]
            app = App("apt", pkg, (cur["name"] if cur else doc.get("Name")) or pkg,
                      rank=0 if cur and cur["featured"] else (1 if cur else 3),
                      summary=(cur["desc"] if cur else "") or doc.get("Summary", ""),
                      category=cur["cat"] if cur else category_of(doc.get("Categories", [])),
                      publisher=(cur["pub"] if cur else "") or doc.get("DeveloperName", ""),
                      icon=doc.get("icon", ""), featured=bool(cur and cur["featured"]),
                      version=ver, available=upgradable.get(pkg, cands.get(pkg, ver)),
                      state="u" if pkg in upgradable else ("i" if ver else "a"),
                      appstream_id=doc.get("ID", "").removesuffix(".desktop").lower())
            out.append(app)
        return out

    def details(self, app):
        code, out = run(["apt-cache", "show", "--no-all-versions", app.ref], 30, env={**os.environ, "LANG": "C"})
        info, desc, in_desc = {}, [], False
        for line in out.splitlines():
            if in_desc and line.startswith(" "):
                text = line.strip()
                desc.append("" if text == "." else text)
                continue
            in_desc = False
            if ":" in line:
                k, v = line.split(":", 1)
                info[k] = v.strip()
                if k == "Description":
                    in_desc = True
        doc = dep11_index().get(app.ref) or {}
        if doc:
            app.description = doc["description"] or app.description
            app.screenshots = doc["screenshots"]
            app.releases = [tuple(r) for r in doc["releases"]]
            app.license = doc["ProjectLicense"]
            app.homepage = doc["homepage"]
        if not app.description:
            paras = " ".join(desc).split("  ")
            app.description = [p.strip() for p in re.split(r"\s{2,}|\n", "\n".join(desc)) if p.strip()] or paras
        app.homepage = app.homepage or info.get("Homepage", "")
        app.publisher = app.publisher or info.get("Maintainer", "").split("<")[0].strip()
        app.available = app.available or info.get("Version", "")
        if info.get("Installed-Size", "").isdigit():
            app.size_kb = int(info["Installed-Size"])
        if not app.license:
            app.license = self.copyright(app.ref)
        if not app.releases:
            app.releases = self.changelog(app.ref)
        return app

    def more_screenshots(self, app):
        """Captures de screenshots.debian.net pour les paquets sans AppStream."""
        data = fetch(f"https://screenshots.debian.net/json/package/{urllib.parse.quote(app.ref)}", 8)
        try:
            shots = json.loads(data or b"{}").get("screenshots", [])
            return [s.get("large_image_url") or s.get("small_image_url") for s in shots][:5]
        except (ValueError, AttributeError):
            return []

    def copyright(self, pkg):
        try:
            text = Path(f"{ROOT}/usr/share/doc/{pkg}/copyright").read_text(errors="replace")
        except OSError:
            return ""
        m = re.search(r"^License:\s*(\S.*)$", text, re.M)
        return m.group(1).strip() if m else ""

    def changelog(self, pkg):
        path = Path(f"{ROOT}/usr/share/doc/{pkg}/changelog.Debian.gz")
        try:
            text = gzip.decompress(path.read_bytes()).decode("utf-8", "replace")
        except (OSError, EOFError):
            return []
        out = []
        for m in re.finditer(r"^\S+ \(([^)]+)\)[^\n]*\n(.*?)^ -- [^\n]*?(\w{3}, \d+ \w{3} \d{4})", text, re.M | re.S):
            items = [l.strip(" *-") for l in m.group(2).splitlines() if l.strip().startswith("*")]
            out.append((m.group(1), m.group(3), " ; ".join(items)[:300]))
            if len(out) >= 5:
                break
        return out

    def apt(self, args, progress):
        r, w = os.pipe()
        cmd = admin(["apt-get", "-y", "-q", "-o", "DPkg::Lock::Timeout=120", "-o", "Dpkg::Use-Pty=0",
                     "-o", f"APT::Status-Fd={w}", *args])
        if cmd is None:
            os.close(r)
            os.close(w)
            return 126
        env = {**os.environ, "DEBIAN_FRONTEND": "noninteractive"}
        with open(RUN / "store.log", "ab") as log:
            try:
                proc = subprocess.Popen(cmd, stdout=log, stderr=log, stdin=subprocess.DEVNULL,
                                        pass_fds=(w,), env=env)
            except OSError:
                os.close(r)
                os.close(w)
                return 127
            os.close(w)
            with os.fdopen(r, "r", errors="replace") as status:
                for line in status:
                    parts = line.rstrip("\n").split(":", 3)
                    if len(parts) == 4 and parts[0] in ("dlstatus", "pmstatus"):
                        try:
                            pct = int(float(parts[2]))
                        except ValueError:
                            pct = 0
                        progress(pct // 2 if parts[0] == "dlstatus" else 50 + pct // 2, parts[3])
            return proc.wait()

    def install(self, ref, progress):
        return self.apt(["install", "--no-install-recommends", "--", ref], progress)

    def remove(self, ref, progress):
        return self.apt(["remove", "--", ref], progress)

    def update(self, refs, progress):
        return self.apt(["install", "--only-upgrade", "--", *refs], progress)

    def update_all(self, progress):
        return self.apt(["upgrade"], progress)


class Flatpak(Source):
    name, label = "flatpak", "Flathub"

    def remotes(self):
        code, out = run(["flatpak", "remotes", "--columns=name,options"], 20)
        return [l.split("\t")[0].strip() for l in out.splitlines() if l.strip() and "disabled" not in l]

    def available(self):
        return have("flatpak") and bool(self.remotes())

    def appstream_dirs(self, remote):
        for base in (Path(ROOT + "/var/lib/flatpak"), DATA / "flatpak"):
            d = base / "appstream" / remote / "x86_64" / "active"
            if d.is_dir():
                yield d

    def installed(self):
        code, out = run(["flatpak", "list", "--app", "--columns=application,version,origin"], 30)
        res = {}
        for line in out.splitlines():
            p = line.split("\t")
            if p and p[0]:
                res[p[0]] = p[1] if len(p) > 1 and p[1] else tr("installed_version")
        return res

    def updates(self):
        code, out = run(["flatpak", "remote-ls", "--updates", "--app", "--columns=application,version"], 60)
        return {p[0]: (p[1] if len(p) > 1 else "") for p in (l.split("\t") for l in out.splitlines()) if p and p[0]}

    def apps(self, force=False):
        if force and not OFFLINE:
            run(["flatpak", "update", "--appstream", "-y", "--noninteractive"], 300)
        installed, updates = self.installed(), self.updates()
        out, seen = [], set()
        for remote in self.remotes():
            for d in self.appstream_dirs(remote):
                xmlf = next((p for p in (d / "appstream.xml.gz", d / "appstream.xml") if p.exists()), None)
                if xmlf is None:
                    continue
                for c in appstream_xml(xmlf.read_bytes()):
                    if c["ref"] in seen:
                        continue
                    seen.add(c["ref"])
                    icon = ""
                    for h, name in sorted(c["icons"].get("cached", []), reverse=True):
                        p = d / "icons" / f"{h}x{h}" / name
                        if p.is_file():
                            icon = str(p)
                            break
                    if not icon:
                        icon = next((n for _, n in c["icons"].get("remote", [])), "")
                    ver = installed.get(c["ref"], "")
                    latest = c["releases"][0][0] if c["releases"] else ""
                    app = App("flatpak", c["ref"], c["name"], summary=c["summary"], description=c["description"],
                              category=category_of(c["categories"]), publisher=c["publisher"],
                              license=c["license"], homepage=c["homepage"], icon=icon,
                              screenshots=c["screenshots"], releases=c["releases"],
                              version=ver, available=updates.get(c["ref"]) or latest or ver,
                              state="u" if c["ref"] in updates else ("i" if ver else "a"),
                              appstream_id=c["id"].removesuffix(".desktop").lower())
                    app.remote = remote
                    out.append(app)
        return out

    def flatpak(self, args, progress):
        progress(5, "Flatpak...")
        with open(RUN / "store.log", "ab") as log:
            try:
                proc = subprocess.Popen(["flatpak", *args, "-y", "--noninteractive"], stdout=subprocess.PIPE,
                                        stderr=log, stdin=subprocess.DEVNULL)
            except OSError:
                return 127
            for raw in iter(lambda: proc.stdout.readline(), b""):
                text = raw.decode("utf-8", "replace").strip()
                m = re.search(r"(\d{1,3})%", text)
                if m:
                    progress(min(99, int(m.group(1))), text[:120])
                log.write(raw)
            return proc.wait()

    def install(self, ref, progress):
        remote = next(iter(self.remotes()), "flathub")
        return self.flatpak(["install", remote, ref], progress)

    def remove(self, ref, progress):
        return self.flatpak(["uninstall", ref], progress)

    def update(self, refs, progress):
        return self.flatpak(["update", *refs], progress)

    def update_all(self, progress):
        return self.flatpak(["update"], progress)


class SnapdClient(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost", timeout=20)
        self.path = path

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(20)
        self.sock.connect(self.path)


class Snap(Source):
    name, label = "snap", "Snap Store"
    SOCKET = ROOT + "/run/snapd.socket"
    SECTIONS = ["featured", "productivity", "development", "games", "photo-and-video",
                "music-and-audio", "utilities", "art-and-design"]

    def available(self):
        return have("snap") and os.path.exists(self.SOCKET)

    def api(self, path):
        try:
            conn = SnapdClient(self.SOCKET)
            conn.request("GET", path)
            data = json.loads(conn.getresponse().read() or b"{}")
            conn.close()
            return data.get("result")
        except (OSError, ValueError, http.client.HTTPException):
            return None

    def apps(self, force=False):
        cache = CACHE / "remote" / "snap.json"
        found = load_json(cache, {}) if cache.exists() else {}
        if force or not found or time.time() - cache.stat().st_mtime > REMOTE_TTL:
            fresh = {}
            for section in self.SECTIONS:
                for s in self.api(f"/v2/find?section={section}") or []:
                    s["_section"] = section
                    fresh.setdefault(s.get("name"), s)
            if fresh:
                found = fresh
                atomic_write(cache, json.dumps(found))
        installed = {s.get("name"): s for s in self.api("/v2/snaps") or []}
        code, out = run(["snap", "refresh", "--list"], 60)
        updates = {l.split()[0]: l.split()[1] for l in out.splitlines()[1:] if len(l.split()) > 1}
        out_apps = []
        for name, s in {**found, **{k: v for k, v in installed.items() if k not in found}}.items():
            if not name or s.get("type", "app") != "app":
                continue
            medias = s.get("media") or []
            ver = (installed.get(name) or {}).get("version", "")
            app = App("snap", name, s.get("title") or name, summary=s.get("summary", ""),
                      description=[p for p in (s.get("description") or "").split("\n") if p.strip()],
                      category=category_of([s.get("_section", "")] + [c.get("name", "") for c in s.get("categories") or []]),
                      publisher=(s.get("publisher") or {}).get("display-name", ""), license=s.get("license", ""),
                      homepage=s.get("website", "") or s.get("store-url", ""),
                      icon=next((m["url"] for m in medias if m.get("type") == "icon"), ""),
                      screenshots=[m["url"] for m in medias if m.get("type") == "screenshot"][:6],
                      version=ver, available=updates.get(name) or s.get("version", ""),
                      size_kb=int(s.get("download-size", 0) or 0) // 1024,
                      state="u" if name in updates else ("i" if ver else "a"),
                      featured=s.get("_section") == "featured")
            app.classic = s.get("confinement") == "classic"
            out_apps.append(app)
        return out_apps

    def snap(self, args, progress):
        cmd = admin(["snap", *args])
        if cmd is None:
            return 126
        progress(10, "Snap Store...")
        with open(RUN / "store.log", "ab") as log:
            try:
                return subprocess.run(cmd, stdout=log, stderr=log, stdin=subprocess.DEVNULL, timeout=1800).returncode
            except (OSError, subprocess.SubprocessError):
                return 127

    def install(self, ref, progress):
        cached = load_json(CACHE / "remote" / "snap.json", {}).get(ref, {})
        return self.snap(["install", ref] + (["--classic"] if cached.get("confinement") == "classic" else []), progress)

    def remove(self, ref, progress):
        return self.snap(["remove", ref], progress)

    def update(self, refs, progress):
        return self.snap(["refresh", *refs], progress)

    def update_all(self, progress):
        return self.snap(["refresh"], progress)


class AppImage(Source):
    """Catalogue AppImageHub ; les AppImage vont dans ~/Applications."""
    name, label = "appimage", "AppImage"
    FEED = "https://appimage.github.io/feed.json"
    MEDIA = "https://appimage.github.io/database/"

    def __init__(self):
        self.feed = None

    def load(self, force=False):
        if self.feed is None or force:
            data = cached_fetch(os.environ.get("WINLUX_APPIMAGE_FEED", self.FEED), "appimage.json", force=force)
            try:
                self.feed = json.loads(data).get("items", []) if data else []
            except ValueError:
                self.feed = []
        return self.feed

    def available(self):
        return bool(self.load())

    def apps(self, force=False):
        state = load_json(STATE, {}).get("appimage", {})
        out = []
        for item in self.load(force):
            name = item.get("name", "")
            github = next((l.get("url") for l in item.get("links") or [] if l.get("type") == "GitHub"), "")
            if not name or not github:
                continue
            ref = re.sub(r"[^A-Za-z0-9._-]", "_", name)
            ver = (state.get(ref) or {}).get("version", "")
            icons = item.get("icons") or []
            app = App("appimage", ref, name, summary=(item.get("description") or "").split(".")[0][:140],
                      description=[item.get("description", "")], category=category_of(item.get("categories") or []),
                      publisher=", ".join(a.get("name", "") for a in item.get("authors") or []),
                      license=item.get("license") or "", homepage=f"https://github.com/{github}",
                      icon=self.MEDIA + icons[0] if icons else "",
                      screenshots=[self.MEDIA + s for s in item.get("screenshots") or []][:5],
                      version=ver, state="i" if ver else "a")
            app.github = github
            out.append(app)
        return out

    def latest(self, github):
        data = fetch(f"https://api.github.com/repos/{github}/releases/latest", 20)
        try:
            rel = json.loads(data or b"{}")
        except ValueError:
            return None, None
        for asset in rel.get("assets") or []:
            n = asset.get("name", "")
            if n.lower().endswith(".appimage") and not re.search(r"(arm|aarch64|i386|i686)", n, re.I):
                return rel.get("tag_name", ""), asset.get("browser_download_url")
        return None, None

    def details(self, app):
        if getattr(app, "github", "") and not app.available:
            app.available = self.latest(app.github)[0] or ""
        return app

    def install(self, ref, progress):
        app = next((a for a in self.apps() if a.ref == ref), None)
        if app is None:
            return 2
        progress(5, tr("latest"))
        tag, url = self.latest(app.github)
        if not url:
            return 3
        data = fetch(url, 300, 2 << 30)
        if not data or not data[:4] == b"\x7fELF":
            return 4
        dest = HOME / "Applications" / f"{ref}.AppImage"
        dest.parent.mkdir(parents=True, exist_ok=True)
        tmp = dest.with_suffix(".part")
        tmp.write_bytes(data)
        tmp.chmod(0o755)
        os.replace(tmp, dest)
        icon = media(app.icon, "icons")
        desktop = DATA / "applications" / f"winlux-appimage-{ref}.desktop"
        desktop.parent.mkdir(parents=True, exist_ok=True)
        desktop.write_text(f"[Desktop Entry]\nType=Application\nName={app.name}\nExec=\"{dest}\"\n"
                           f"Icon={icon}\nTerminal=false\nCategories=Utility;\nX-Winlux-Store={app.key}\n")
        state = load_json(STATE, {})
        state.setdefault("appimage", {})[ref] = {"version": tag, "path": str(dest)}
        save_state(state)
        return 0

    def remove(self, ref, progress):
        state = load_json(STATE, {})
        entry = state.get("appimage", {}).pop(ref, None)
        if entry is None:
            return 2
        for p in (Path(entry.get("path", "")), DATA / "applications" / f"winlux-appimage-{ref}.desktop"):
            if p.is_file():
                p.unlink()
        save_state(state)
        return 0

    def update(self, refs, progress):
        return max((self.install(r, progress) for r in refs), default=0)

    def update_all(self, progress):
        return self.update(list(load_json(STATE, {}).get("appimage", {})), progress)


class WindowsCatalog:
    """Fournisseur de catalogue Windows interchangeable.

    Chaque fichier JSON de store-sources.d déclare une source :
      {"id": "winlux", "name": "Catalogue Winlux", "format": "winlux",
       "url": "https://.../catalog.json"}   (ou "file": "/chemin/catalog.json")
    Formats livrés : « winlux » (docs/STORE.md) et « scoop » (archive .tar.gz
    d'un dépôt de manifestes Scoop). D'autres formats s'ajoutent en
    enregistrant un adaptateur dans FORMATS (octets -> fiches au format
    winlux), sans toucher au reste du Store.
    """
    FORMATS = {}

    def __init__(self, conf):
        self.id = re.sub(r"[^a-z0-9_-]", "", str(conf.get("id", "")).lower()) or "catalogue"
        self.name = conf.get("name", self.id)
        self.conf = conf

    def raw(self, force=False):
        if self.conf.get("file"):
            try:
                return Path(self.conf["file"]).read_bytes()
            except OSError:
                return None
        if self.conf.get("url"):
            return cached_fetch(self.conf["url"], f"windows-{self.id}.data", force=force)
        return None

    def entries(self, force=False):
        adapter = self.FORMATS.get(self.conf.get("format", "winlux"))
        if adapter is None:
            return []
        data = self.raw(force)
        if not data:
            return []
        # Les fiches converties sont gardées tant que le catalogue ne change pas.
        digest = hashlib.sha256(data).hexdigest()[:16]
        memo = CACHE / "remote" / f"windows-{self.id}.entries.json"
        cached = load_json(memo, None)
        if isinstance(cached, dict) and cached.get("digest") == digest:
            return cached.get("apps", [])
        try:
            apps = [a for a in adapter(data) if valid_windows_entry(a)]
        except (ValueError, TypeError, AttributeError, KeyError, OSError, EOFError, tarfile.TarError):
            return []
        memo.parent.mkdir(parents=True, exist_ok=True)
        atomic_write(memo, json.dumps({"digest": digest, "apps": apps}, ensure_ascii=False))
        return apps


INSTALLER_TYPES = ("exe", "msi", "zip", "portable")

def valid_windows_entry(a):
    """Une fiche sans empreinte SHA-256 (ou SHA-512) n'est jamais installée."""
    inst = a.get("installer") if isinstance(a, dict) else None
    if not (isinstance(inst, dict) and a.get("id") and a.get("name") and inst.get("url")):
        return False
    if inst.get("type", "exe") not in INSTALLER_TYPES:
        return False
    return bool(re.fullmatch(r"[0-9a-fA-F]{64}", inst.get("sha256", ""))
                or re.fullmatch(r"[0-9a-fA-F]{128}", inst.get("sha512", "")))

def winlux_format(data):
    """Format Winlux : {"apps": [{id, name, version, installer: {url, sha256}}...]}."""
    return json.loads(data).get("apps", [])

WindowsCatalog.FORMATS["winlux"] = winlux_format

WORDS = ((("browser", "navigateur", "web browser"), "navigateurs"),
         (("game", "emulator", "jeu ", "chess", "minecraft"), "jeux"),
         (("editor for code", "code editor", "ide", "developer", "programming", "git ", "git client",
           "compiler", "debugger", "database", "sql", "api ", "json", "regex", "hex editor"), "developpement"),
         (("office", "pdf", "document", "spreadsheet", "note", "markdown", "ebook", "e-book",
           "text editor", "writer", "calendar", "mail", "email"), "bureautique"),
         (("video", "audio", "music", "image", "photo", "media", "player", "paint", "drawing",
           "graphic", "3d", "screenshot", "recorder", "podcast", "stream"), "multimedia"))

def category_from_text(text):
    text = f" {text.lower()} "
    for words, cat in WORDS:
        if any(w in text for w in words):
            return cat
    return "utilitaires"

def scoop_format(data):
    """Dépôt Scoop (archive .tar.gz de manifestes JSON), applications
    graphiques 64 bits seulement : un fichier .zip, .msi ou .exe portable
    avec empreinte, et sans script PowerShell d'installation."""
    out = []
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:*") as tar:
        for count, member in enumerate(tar):
            if count > 20000:
                break
            if not (member.isfile() and member.name.endswith(".json") and "/bucket/" in member.name
                    and member.size < 512 * 1024):
                continue
            try:
                m = json.loads(tar.extractfile(member).read().decode("utf-8-sig"))
            except (ValueError, UnicodeError, AttributeError):
                continue
            entry = scoop_entry(Path(member.name).stem, m)
            if entry:
                out.append(entry)
    return out

def scoop_entry(app_id, m):
    if not isinstance(m, dict):
        return None
    arch = (m.get("architecture") or {}).get("64bit")
    if not isinstance(arch, dict):
        return None  # le runtime Winlux n'exécute que des programmes 64 bits

    def get(key):
        return arch.get(key, m.get(key))

    if any(get(k) for k in ("installer", "pre_install", "depends", "uninstaller")):
        return None  # scripts PowerShell non exécutés
    url, digest, shortcuts = get("url"), get("hash"), get("shortcuts")
    if isinstance(url, list):
        if len(url) != 1:
            return None
        url, digest = url[0], digest[0] if isinstance(digest, list) else digest
    if not (isinstance(url, str) and isinstance(digest, str) and isinstance(shortcuts, list) and shortcuts):
        return None
    kind, algo = digest.split(":", 1) if ":" in digest else ("sha256", digest)
    if kind not in ("sha256", "sha512"):
        return None
    name_part = url.split("#/")[-1].split("?")[0]
    ext = name_part.rsplit(".", 1)[-1].lower() if "." in name_part.rsplit("/", 1)[-1] else ""
    if get("innosetup"):
        return None  # installateurs Inno Setup 32 bits
    itype = {"zip": "zip", "msi": "msi", "exe": "portable"}.get(ext)
    if not itype:
        return None
    first = shortcuts[0] if isinstance(shortcuts[0], list) else []
    name = first[1] if len(first) > 1 and isinstance(first[1], str) else app_id
    license_ = m.get("license")
    if isinstance(license_, dict):
        license_ = license_.get("identifier", "")
    desc = str(m.get("description", ""))
    homepage = str(m.get("homepage", ""))
    parsed = urllib.parse.urlparse(homepage)
    publisher = parsed.hostname or ""
    if publisher in ("github.com", "gitlab.com", "codeberg.org", "sourceforge.net"):
        parts = [p for p in parsed.path.split("/") if p]
        publisher = parts[1] if publisher == "sourceforge.net" and len(parts) > 1 else (parts[0] if parts else publisher)
    return {
        "id": app_id, "name": name, "version": str(m.get("version", "")),
        "summary": desc[:160], "description": [desc] if desc else [],
        "category": category_from_text(f"{name} {desc}"),
        "publisher": publisher.removeprefix("www."), "license": str(license_ or ""),
        "homepage": homepage,
        "releases": [{"version": str(m.get("version", ""))}],
        "installer": {
            "url": url.split("#")[0], kind: algo.lower(), "type": itype,
            "file": Path(name_part).name or f"{app_id}.{ext}",
            "extract_dir": str(get("extract_dir") or ""),
            "shortcuts": [s[:3] for s in shortcuts if isinstance(s, list) and len(s) >= 2
                          and all(isinstance(x, str) for x in s[:3])][:8],
        },
    }

WindowsCatalog.FORMATS["scoop"] = scoop_format


def pe_is_64bit(path):
    """Vrai pour un exécutable PE32+ x86-64 (le seul que le runtime exécute)."""
    try:
        with open(path, "rb") as f:
            head = f.read(4096)
        off = int.from_bytes(head[0x3C:0x40], "little")
        return head[:2] == b"MZ" and head[off:off + 4] == b"PE\0\0" and head[off + 4:off + 6] == b"\x64\x86"
    except (OSError, ValueError):
        return False

def write_shortcut(path, target, args="", workdir=""):
    """Raccourci .lnk minimal (lu par l'Explorateur et winlux-applications)."""
    flags = 0x08 | 0x80 | (0x10 if workdir else 0) | (0x20 if args else 0)
    head = struct.pack("<I16sIIQQQIiIHHII", 0x4C,
                       bytes.fromhex("0114020000000000C000000000000046"),
                       flags, 0x20, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0)
    body = b""
    for s in (target, workdir, args):
        if s:
            raw = s.encode("utf-16-le")
            body += struct.pack("<H", len(raw) // 2) + raw
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(head + body)

def windows_path(path):
    drive_c = Path(os.environ.get("WINLUX_DRIVE_C", HOME))
    try:
        return "C:\\" + str(path.relative_to(drive_c)).replace("/", "\\")
    except ValueError:
        return "Z:" + str(path).replace("/", "\\")

DRIVE_C = Path(os.environ.get("WINLUX_DRIVE_C", HOME))
START_MENU = DRIVE_C / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs"

def safe_name(name):
    return re.sub(r'[<>:"/\\|?*\x00-\x1f]', "_", name).strip(". ") or "Application"


class Windows(Source):
    name, label = "windows", "Windows"

    def providers(self):
        found = []
        for d in CONFIG_DIRS:
            for f in sorted(d.glob("*.json")) if d.is_dir() else []:
                conf = load_json(f, None)
                if isinstance(conf, dict) and conf.get("enabled", True):
                    found.append(WindowsCatalog(conf))
        return found

    def available(self):
        return have("winlux") and any(p.entries() for p in self.providers())

    def apps(self, force=False):
        state = load_json(STATE, {}).get("windows", {})
        out = []
        for prov in self.providers():
            for a in prov.entries(force):
                ref = f"{prov.id}/{re.sub(r'[^A-Za-z0-9._-]', '_', a['id'])}"
                ver = (state.get(ref) or {}).get("version", "")
                latest = str(a.get("version", ""))
                app = App("windows", ref, a["name"], summary=a.get("summary", ""),
                          description=a.get("description") if isinstance(a.get("description"), list)
                          else [a.get("description", "")],
                          category=a.get("category") if a.get("category") in CATEGORIES else category_of(a.get("categories") or []),
                          publisher=a.get("publisher", ""), license=a.get("license", ""),
                          homepage=a.get("homepage", ""), icon=a.get("icon", ""),
                          screenshots=a.get("screenshots", [])[:6],
                          releases=[(r.get("version", ""), r.get("date", ""), r.get("notes", ""))
                                    for r in a.get("releases", []) if isinstance(r, dict)][:8],
                          version=ver, available=latest, size_kb=int(a.get("size_kb", 0) or 0),
                          state="u" if ver and latest and ver != latest else ("i" if ver else "a"),
                          featured=bool(a.get("featured")))
                app.installer, app.provider = a["installer"], prov.name
                out.append(app)
        return out

    def install(self, ref, progress):
        app = next((a for a in self.apps() if a.ref == ref), None)
        if app is None:
            return 2
        inst = app.installer
        progress(5, tr("download", name=app.name))
        data = fetch(inst["url"], 600, 4 << 30)
        if inst.get("sha512"):
            ok = data is not None and hashlib.sha512(data).hexdigest() == inst["sha512"].lower()
        else:
            ok = data is not None and hashlib.sha256(data).hexdigest() == inst.get("sha256", "").lower()
        if not ok:
            progress(0, tr("sha"))
            return 5 if data is not None else 7
        kind = inst.get("type", "exe")
        ext = {"msi": ".msi", "zip": ".zip"}.get(kind, ".exe")
        if kind == "exe" and inst["url"].lower().endswith(".msi"):
            kind, ext = "msi", ".msi"
        path = CACHE / "installers" / f"{ref.replace('/', '_')}{ext}"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        state = load_json(STATE, {})
        old = state.get("windows", {}).get(ref)
        if old and old.get("files"):
            remove_files(old)  # mise à jour d'une application portable
        entry = {"version": app.available, "name": app.name}
        if kind in ("zip", "portable"):
            code = install_portable(app, path, kind, entry, progress)
        else:
            code = install_windows_file(path, progress, [str(x) for x in inst.get("args", [])], entry)
        if code == 0:
            state = load_json(STATE, {})
            state.setdefault("windows", {})[ref] = entry
            save_state(state)
        return code

    def remove(self, ref, progress):
        state = load_json(STATE, {})
        entry = state.get("windows", {}).get(ref)
        if entry is None:
            return 2
        if entry.get("msi"):
            code = run(["winlux", "--msiexec", "/x", entry["msi"], "/qn"], 1800)[0]
            if code not in (0, 1605 & 0xFF):
                return code
        elif entry.get("files"):
            remove_files(entry)
        else:
            uninstaller = entry.get("uninstaller")
            if uninstaller and Path(uninstaller).is_file():
                code = run(["winlux", uninstaller], 1800)[0]
                if code != 0:
                    return code
        state["windows"].pop(ref, None)
        save_state(state)
        if have("winlux-applications"):
            run(["winlux-applications", "refresh"], 120)
        return 0

    def update(self, refs, progress):
        return max((self.install(r, progress) for r in refs), default=0)

    def update_all(self, progress):
        apps = [a.ref for a in self.apps() if a.state == "u"]
        return self.update(apps, progress) if apps else 0


def remove_files(entry):
    for f in entry.get("shortcuts", []):
        try:
            Path(f).unlink()
        except OSError:
            pass
    target = entry.get("files")
    if target and Path(target).is_dir() and DRIVE_C in Path(target).parents:
        shutil.rmtree(target, ignore_errors=True)

def install_portable(app, path, kind, entry, progress):
    """Application portable (.zip ou .exe) : C:\\Program Files\\<nom> et
    raccourcis du menu Démarrer, sans exécuter d'installateur."""
    inst = app.installer
    dest = DRIVE_C / "Program Files" / safe_name(app.name)
    tmp = dest.with_name(dest.name + ".winlux-tmp")
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    progress(40, tr("installing", name=app.name))
    try:
        if kind == "zip":
            with zipfile.ZipFile(path) as z:
                total = 0
                for info in z.infolist():
                    total += info.file_size
                    target = (tmp / info.filename).resolve()
                    if total > 8 << 30 or tmp.resolve() not in target.parents and target != tmp.resolve():
                        raise ValueError("archive refusée")
                z.extractall(tmp)
            sub = inst.get("extract_dir", "").strip("/\\")
            root = tmp / sub.replace("\\", "/") if sub else tmp
            if not root.is_dir():
                raise ValueError("dossier extract_dir absent")
        else:
            root = tmp
            shutil.copy2(path, tmp / safe_name(inst.get("file") or f"{app.ref.split('/')[-1]}.exe"))
        shortcuts = inst.get("shortcuts") or []
        exes = [root / s[0].replace("\\", "/") for s in shortcuts]
        if exes and not any(pe_is_64bit(e) for e in exes):
            progress(0, tr("win32"))
            return 8
        shutil.rmtree(dest, ignore_errors=True)
        root.rename(dest)
    except (OSError, ValueError, zipfile.BadZipFile) as e:
        progress(0, tr("install_failed", error=e))
        return 1
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    links = []
    for s in inst.get("shortcuts") or []:
        exe = dest / s[0].replace("\\", "/")
        if not exe.is_file():
            continue
        link = START_MENU / f"{safe_name(s[1])}.lnk"
        write_shortcut(link, windows_path(exe), s[2] if len(s) > 2 else "", windows_path(exe.parent))
        links.append(str(link))
    entry.update({"files": str(dest), "shortcuts": links})
    if have("winlux-applications"):
        run(["winlux-applications", "refresh"], 120)
    return 0

def install_windows_file(path, progress, args=(), entry=None):
    """Lance un installateur .exe, ou le msiexec intégré au runtime pour un .msi."""
    progress(15, tr("runtime", name=path.name))
    msi = path.suffix.lower() == ".msi"
    if msi:
        cmd = ["winlux", "--msiexec", "/i", str(path), "/qn", *args]
    else:
        cmd = ["winlux", str(path), *args]
    with open(RUN / "store.log", "ab") as log:
        try:
            code = subprocess.run(cmd, cwd=str(path.parent), stdout=log, stderr=log,
                                  stdin=subprocess.DEVNULL).returncode
        except OSError:
            code = 127
    if code == 0 and msi and entry is not None:
        info = run(["winlux", "--msiexec", "--info", str(path)], 60)
        product = info[1].split("\t")[0].strip() if info[0] == 0 and info[1] else ""
        if product.startswith("{"):
            entry["msi"] = product
    if have("winlux-applications"):
        run(["winlux-applications", "refresh"], 120)
    return code


SOURCES = [Apt(), Flatpak(), Snap(), AppImage(), Windows()]
SOURCE_BY_NAME = {s.name: s for s in SOURCES}
PRIORITY = {"apt": 0, "flatpak": 1, "snap": 2, "appimage": 3, "windows": 4}

# ---------------------------------------------------------------------------
# Index de la bibliothèque
# ---------------------------------------------------------------------------

def norm(name):
    return re.sub(r"[^a-z0-9]", "", name.lower())

def merge(apps):
    """Une entrée par logiciel : l'ID AppStream ou le nom rapprochent les
    sources ; la source installée, sinon la plus intégrée (apt), est choisie."""
    groups = {}
    for a in apps:
        key = a.appstream_id or norm(a.name)
        alias = norm(a.name)
        group = groups.get(key) or groups.get(alias)
        if group is None:
            group = []
        group.append(a)
        groups[key] = groups[alias] = group
    unique, seen = [], set()
    for group in groups.values():
        if id(group) in seen:
            continue
        seen.add(id(group))
        group.sort(key=lambda a: (a.state == "a", PRIORITY.get(a.source, 9)))
        main = group[0]
        main.alternatives = [a.key for a in group[1:]]
        main.featured = any(a.featured for a in group)
        main.rank = min(a.rank for a in group)
        if not main.icon:
            main.icon = next((a.icon for a in group if a.icon), "")
        unique.append(main)
    return unique

def badge(app):
    if app.source == "windows":
        return getattr(app, "provider", "Windows")
    return SOURCE_BY_NAME[app.source].label

def collect(force=False, only_local=False):
    sources = [s for s in SOURCES if (not only_local or s.name in ("apt", "flatpak")) and s.available()]
    SOURCE_BY_NAME["apt"].local_only = only_local
    apps = []
    for s in sources:
        try:
            apps += s.apps(force)
        except Exception as exc:  # une source défaillante ne bloque pas les autres
            with open(RUN / "store.log", "a") as log:
                log.write(f"source {s.name}: {exc}\n")
    return sources, merge(apps)

def local_icon(app, download):
    icon = app.icon
    if icon.startswith("/"):
        return icon if icon.lower().endswith(".png") and os.path.exists(icon) else ""
    if icon.startswith("http"):
        digest = hashlib.sha256(icon.encode()).hexdigest()[:24]
        path = CACHE / "icons" / f"{digest}.png"
        if path.exists():
            return str(path)
        return media(icon, "icons") if download else ""
    return ""

def write_index(sources, apps, refreshing, download=False):
    apps.sort(key=lambda a: (not a.featured, a.state == "a", not a.icon, a.name.lower()))
    apps = apps[:MAX_INDEX]
    if download:
        todo = [a for a in apps if a.icon.startswith("http")][:400]
        lock = threading.Lock()
        def worker():
            while True:
                with lock:
                    if not todo:
                        return
                    a = todo.pop(0)
                local_icon(a, True)
        threads = [threading.Thread(target=worker) for _ in range(8)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
    apt = SOURCE_BY_NAME["apt"]
    lines = [f"sources={','.join(s.name for s in sources)}", f"refreshing={int(refreshing)}",
             f"updated={int(time.time())}",
             f"updates={sum(1 for a in apps if a.state == 'u')}",
             f"system_updates={getattr(apt, 'system_updates', 0)}"]
    for a in sorted(apps, key=lambda a: (0 if a.featured else a.rank, a.state == "a" and not local_icon(a, False),
                                         a.name.lower())):
        lines.append("app=" + "|".join(field(v, 160) for v in (
            a.key, a.category, a.name, a.summary, a.publisher, a.state, a.version, a.available,
            a.size_kb, local_icon(a, False), badge(a), int(a.featured), ",".join(a.alternatives))))
    apps_dir = DATA / "applications"
    for desktop in sorted(apps_dir.glob("winlux-pe-*.desktop")) if apps_dir.is_dir() else []:
        text = desktop.read_text(errors="replace")
        name = re.search(r"^Name=(.*)$", text, re.M)
        exe = re.search(r"^X-Winlux-Executable=(.*)$", text, re.M)
        if name:
            lines.append(f"win={field(name.group(1))}|{field(exe.group(1) if exe else '')}")
    atomic_write(RUN / "store.txt", "\n".join(lines) + "\n")
    atomic_write(CACHE / "index.json", json.dumps([a.__dict__ for a in apps], default=str))

def refresh(force=False):
    lock = RUN / "store-refresh.lock"
    try:
        lock.mkdir(parents=True)
    except FileExistsError:
        if time.time() - lock.stat().st_mtime < 900:
            return 0
    try:
        # 1. Sources locales tout de suite, 2. tout (réseau, vignettes) ensuite.
        sources, apps = collect(False, only_local=True)
        write_index(sources, apps, True)
        sources, apps = collect(force)
        write_index(sources, apps, True, download=True)
        write_index(sources, apps, False)
    finally:
        lock.rmdir()
    return 0

# ---------------------------------------------------------------------------
# Fiche détaillée et opérations
# ---------------------------------------------------------------------------

def find(key):
    """Logiciel d'après sa clé : depuis l'index en cache (rapide), sinon en
    interrogeant la source."""
    source, _, ref = key.partition(":")
    src = SOURCE_BY_NAME.get(source)
    if src is None or not ref:
        return None, None
    for entry in load_json(CACHE / "index.json", []):
        if entry.get("source") == source and entry.get("ref") == ref:
            app = App(source, ref, entry.get("name", ref))
            app.__dict__.update(entry)
            return src, app
    if not src.available():
        return None, None
    return src, next((a for a in src.apps() if a.ref == ref), None)

def details(key):
    src, app = find(key)
    if app is None:
        atomic_write(RUN / "store-detail.txt", f"key={field(key)}\nerror=1\n")
        return 2
    app = src.details(app)
    index = {a.get("source") + ":" + a.get("ref"): a for a in load_json(CACHE / "index.json", [])}
    alts = index.get(key, {}).get("alternatives", [])
    lines = [f"key={field(app.key)}", f"name={field(app.name)}", f"summary={field(app.summary)}",
             f"publisher={field(app.publisher)}", f"license={field(app.license)}",
             f"version={field(app.version)}", f"available={field(app.available)}",
             f"size_kb={int(app.size_kb or 0)}", f"homepage={field(app.homepage)}",
             f"source={field(badge(app))}", f"state={app.state}", f"category={app.category}",
             f"icon={local_icon(app, True)}"]
    for p in app.description[:14]:
        lines.append(f"desc={field(p, 700)}")
    lines += [f"release={field(v, 40)}|{field(d, 20)}|{field(t, 300)}" for v, d, t in app.releases[:6]]
    for alt in alts:
        s = SOURCE_BY_NAME.get(alt.split(":", 1)[0])
        lines.append(f"alt={field(alt)}|{s.label if s else alt}")
    # Fiche affichée tout de suite ; les captures (réseau) suivent.
    atomic_write(RUN / "store-detail.txt", "\n".join(lines + ["partial=1"]) + "\n")
    urls = app.screenshots or (src.more_screenshots(app) if hasattr(src, "more_screenshots") else [])
    shots = [m for m in (media(u, "shots") for u in urls[:5]) if m]
    atomic_write(RUN / "store-detail.txt", "\n".join(lines + [f"shot={s}" for s in shots]) + "\n")
    return 0

class Progress:
    def __init__(self, op, target, hook=None):
        self.op, self.target, self.hook = op, target, hook

    def __call__(self, percent, message, state="running"):
        percent = max(0, min(100, int(percent)))
        atomic_write(RUN / "store-progress.txt",
                     f"op={self.op}\ntarget={field(self.target)}\nstate={state}\n"
                     f"percent={percent}\nmessage={field(message, 200)}\n")
        if self.hook:
            self.hook(percent)

MESSAGES = {126: "admin", 7: "offline", 8: "win32", 5: "sha"}

def operate(action, target, hook=None, reindex=True, known=""):
    progress = Progress(action, target, hook)
    lock = RUN / "store.lock"
    try:
        lock.mkdir(parents=True)
    except FileExistsError:
        progress(0, tr("busy"), "error")
        return 1
    try:
        progress(0, tr("preparing"))
        if action == "install-file":
            code = install_file(target, progress)
            name = os.path.basename(target.replace("\\", "/"))
        elif action == "update-all":
            code, name = 0, tr("all_software")
            for s in SOURCES:
                if s.available():
                    code = s.update_all(progress) or code
        else:
            src, app = find(target)
            if app is None and known and action == "install":
                # Paquet hors de l'index (logiciels courants de Bienvenue) : la
                # source l'installe directement d'après sa référence.
                src = SOURCE_BY_NAME.get(target.partition(":")[0])
                app = App(src.name, target.partition(":")[2], known) if src else None
            if app is None:
                progress(0, tr("unknown", target=target), "error")
                return 2
            name = app.name
            fn = {"install": src.install, "remove": src.remove}.get(action)
            code = fn(app.ref, progress) if fn else src.update([app.ref], progress)
        done = {"install": "installed", "remove": "removed", "update": "updated",
                "update-all": "all_updated", "install-file": "installed"}.get(action)
        if code == 0:
            progress(100, tr(done, name=name) if done else name, "done")
        elif code in MESSAGES:
            progress(0, tr(MESSAGES[code]), "error")
        else:
            progress(0, tr("failed", code=code, log=RUN / "store.log"), "error")
        return code
    finally:
        lock.rmdir()
        try:
            if reindex:
                sources, apps = collect(False)
                write_index(sources, apps, False)
        except Exception:
            pass

def linux_path(path):
    if re.match(r"^[A-Za-z]:", path):
        letter, rest = path[0].upper(), path[2:].replace("\\", "/").lstrip("/")
        base = "/" if letter == "Z" else os.environ.get("WINLUX_DRIVE_C", str(HOME))
        return Path(base) / rest
    return Path(path)

def install_file(target, progress):
    path = linux_path(target)
    if not path.is_file():
        progress(0, tr("not_found", path=path), "error")
        return 2
    ext = path.suffix.lower()
    if ext in (".exe", ".msi"):
        return install_windows_file(path, progress)
    if ext == ".deb":
        return SOURCE_BY_NAME["apt"].apt(["install", "--", str(path)], progress)
    if ext == ".appimage":
        dest = HOME / "Applications" / path.name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dest)
        dest.chmod(0o755)
        return 0
    if ext == ".flatpakref" and have("flatpak"):
        return SOURCE_BY_NAME["flatpak"].flatpak(["install", "--from", str(path)], progress)
    return 6

# ---------------------------------------------------------------------------
# Logiciels courants (appli Bienvenue) : installation groupée avec reprise
# ---------------------------------------------------------------------------

# Identifiant Bienvenue -> clés candidates, dans l'ordre de préférence. Un
# logiciel sans aucune clé utilisable sur la machine n'est pas proposé.
# Aucun produit Microsoft ici (règle Winlux).
STARTER = [
    ("firefox", ["apt:firefox", "flatpak:org.mozilla.firefox", "apt:firefox-esr"]),
    ("libreoffice", ["apt:libreoffice", "flatpak:org.libreoffice.LibreOffice"]),
    ("vlc", ["apt:vlc", "flatpak:org.videolan.VLC"]),
    ("thunderbird", ["apt:thunderbird", "flatpak:org.mozilla.Thunderbird"]),
    ("gimp", ["apt:gimp", "flatpak:org.gimp.GIMP"]),
    ("7zip", ["apt:7zip", "apt:p7zip-full"]),
    ("keepassxc", ["apt:keepassxc", "flatpak:org.keepassxc.KeePassXC"]),
    ("geany", ["apt:geany", "flatpak:org.geany.Geany"]),
    ("obs", ["apt:obs-studio", "flatpak:com.obsproject.Studio"]),
    ("notepadpp", ["windows:scoop-extras/notepadplusplus"]),
]
STARTER_IDS = [ident for ident, _ in STARTER]
STARTER_NAMES = {"firefox": "Firefox", "libreoffice": "LibreOffice", "vlc": "VLC", "thunderbird": "Thunderbird",
                 "gimp": "GIMP", "7zip": "7-Zip", "keepassxc": "KeePassXC", "geany": "Geany",
                 "obs": "OBS Studio", "notepadpp": "Notepad++"}
STARTER_QUEUE = DATA / "winlux/starter-queue.txt"


def starter_resolve():
    """Clé retenue et état (i installé, a disponible) de chaque logiciel."""
    apt, fp, win = SOURCE_BY_NAME["apt"], SOURCE_BY_NAME["flatpak"], SOURCE_BY_NAME["windows"]
    apt_ok = apt.available()
    apt_inst = apt.installed() if apt_ok else {}
    pkgs = [k[4:] for _, keys in STARTER for k in keys if k.startswith("apt:")]
    cands = apt.candidates([p for p in pkgs if p not in apt_inst]) if apt_ok else {}
    fp_ok = have("flatpak") and fp.available()
    fp_inst = fp.installed() if fp_ok else {}
    remotes = fp.remotes() if fp_ok else []
    win_state = load_json(STATE, {}).get("windows", {})
    win_refs = None

    def installed(key):
        source, _, ref = key.partition(":")
        return (source == "apt" and ref in apt_inst or source == "flatpak" and ref in fp_inst
                or source == "windows" and bool((win_state.get(ref) or {}).get("version")))

    def available(key):
        nonlocal win_refs
        source, _, ref = key.partition(":")
        if source == "apt":
            # Paquet de transition vers Snap (firefox d'Ubuntu) : inutile sans snapd.
            cand = cands.get(ref, "")
            return bool(cand) and ("snap" not in cand or have("snap"))
        if source == "flatpak" and fp_ok:
            return any(run(["flatpak", "remote-info", "--cached", remote, ref], 20)[0] == 0
                       for remote in remotes)
        if source == "windows":
            if win_refs is None:
                try:
                    win_refs = {a.ref for a in win.apps()} if win.available() else set()
                except Exception:
                    win_refs = set()
            return ref in win_refs
        return False

    icons = {}
    for entry in load_json(CACHE / "index.json", []):
        icon = entry.get("icon", "")
        if icon.startswith("/") and icon.lower().endswith(".png") and os.path.exists(icon):
            icons[f"{entry.get('source')}:{entry.get('ref')}"] = icon
    result = {}
    for ident, keys in STARTER:
        key = next((k for k in keys if installed(k)), None)
        state = "i"
        if key is None:
            key, state = next((k for k in keys if available(k)), None), "a"
        if key:
            result[ident] = (key, state, icons.get(key, ""))
    return result


def starter_queue():
    try:
        return [l.strip() for l in STARTER_QUEUE.read_text().splitlines() if l.strip() in STARTER_IDS]
    except OSError:
        return []


def starter_save_queue(queue):
    if queue:
        atomic_write(STARTER_QUEUE, "\n".join(queue) + "\n")
    elif STARTER_QUEUE.exists():
        STARTER_QUEUE.unlink()


def starter_failed():
    try:
        return set((RUN / "starter-failed.txt").read_text().split())
    except OSError:
        return set()


def starter_worker_alive():
    try:
        pid = int((RUN / "starter.lock" / "pid").read_text())
        return pid == os.getpid() or os.path.exists(f"/proc/{pid}")
    except (OSError, ValueError):
        return False


def starter_write(resolved, current="", percent=0):
    """État relu par l'appli Bienvenue (Z:\\run\\winlux\\starter.txt) :
    app=<id>|<i installé, a disponible, w en attente, r en cours, f échec>|<%>|<icône>|<source>."""
    queue, failed = starter_queue(), starter_failed()
    lines = [f"busy={int(bool(current) or bool(queue) and starter_worker_alive())}"]
    for ident in STARTER_IDS:
        if ident not in resolved:
            continue
        key, state, icon = resolved[ident]
        pct = 100 if state == "i" else 0
        if state != "i":
            if ident == current:
                state, pct = "r", percent
            elif ident in queue:
                state = "w"
            elif ident in failed:
                state = "f"
        source = SOURCE_BY_NAME[key.split(":", 1)[0]].label
        lines.append(f"app={ident}|{state}|{pct}|{field(icon, 250)}|{source}")
    atomic_write(RUN / "starter.txt", "\n".join(lines) + "\n")


def starter_run():
    """Installe la file d'attente une application à la fois. La file vit dans
    le dossier de l'utilisateur : fermée ou interrompue (fin de session), elle
    reprend à la session suivante (« starter --resume »)."""
    lock = RUN / "starter.lock"
    while True:
        try:
            lock.mkdir(parents=True)
        except FileExistsError:
            if starter_worker_alive():
                return 0  # un autre processus traite déjà la file
            shutil.rmtree(lock, ignore_errors=True)
            continue
        (lock / "pid").write_text(str(os.getpid()))
        installed_any = False
        try:
            resolved = starter_resolve()
            starter_write(resolved)
            while True:
                queue = starter_queue()
                if not queue:
                    break
                ident = queue[0]
                entry = resolved.get(ident)
                if entry and entry[1] != "i":
                    # Le Store installe déjà autre chose : on attend son tour.
                    waited = 0
                    while (RUN / "store.lock").exists() and waited < 3600:
                        time.sleep(3)
                        waited += 3
                    starter_write(resolved, ident, 0)
                    code = operate("install", entry[0], hook=lambda pct, i=ident: starter_write(resolved, i, pct),
                                   reindex=False, known=STARTER_NAMES.get(ident, ident))
                    failed = starter_failed()
                    if code == 0:
                        failed.discard(ident)
                        installed_any = True
                    else:
                        failed.add(ident)
                    atomic_write(RUN / "starter-failed.txt", " ".join(sorted(failed)) + "\n")
                starter_save_queue([i for i in starter_queue() if i != ident])
                resolved = starter_resolve()
                starter_write(resolved)
        finally:
            shutil.rmtree(lock, ignore_errors=True)
        # Une demande arrivée pendant la libération du verrou est reprise.
        if not starter_queue():
            break
    starter_write(starter_resolve())
    if installed_any:
        try:
            sources, apps = collect(False)
            write_index(sources, apps, False)
        except Exception:
            pass
    return 0


def starter(args):
    if args == ["--list"]:
        if not starter_worker_alive():
            starter_write(starter_resolve())
        return 0
    if args != ["--resume"]:
        queue = starter_queue()
        queue += [i for i in args if i in STARTER_IDS and i not in queue]
        starter_save_queue(queue)
        failed = starter_failed() - set(args)
        atomic_write(RUN / "starter-failed.txt", " ".join(sorted(failed)) + "\n")
    if not starter_queue():
        return 0
    return starter_run()

def main(argv):
    RUN.mkdir(parents=True, exist_ok=True)
    action = argv[1] if len(argv) > 1 else "refresh"
    target = argv[2] if len(argv) > 2 else ""
    if action == "refresh":
        return refresh("--force" in argv)
    if action == "details":
        return details(target)
    if action == "starter":
        return starter(argv[2:])
    if action == "sources":
        for s in SOURCES:
            print(f"{s.name}\t{int(s.available())}")
        return 0
    if action in ("install", "remove", "update", "install-file") and target or action == "update-all":
        return operate(action, target)
    print(__doc__, file=sys.stderr)
    return 2

if __name__ == "__main__":
    sys.exit(main(sys.argv))
