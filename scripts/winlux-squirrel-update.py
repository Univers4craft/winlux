#!/usr/bin/python3 -I
"""Update.exe de Squirrel, servi en natif par Winlux (#1190).

    winlux-squirrel-update RACINE [arguments d'Update.exe…]

RACINE est le dossier Linux d'Update.exe (%LOCALAPPDATA%\\<Appli>). Les
applis Electron (Claude, Discord…) lancent Update.exe pour se mettre à jour ;
le runtime Winlux reconnaît cet Update.exe et appelle cet outil à sa place.

Écrit d'après la documentation publique de Squirrel.Windows (MIT) et ce
qu'attend l'autoUpdater d'Electron ; aucun code de Squirrel n'est repris.

Commandes prises en charge :
  --checkForUpdate URL    une ligne JSON sur la sortie standard, sans rien
                          télécharger d'autre que l'index RELEASES ;
  --download URL          confirmation visible, puis paquet complet vérifié ;
  --update URL            idem, puis installation dans app-<version> ;
  --processStart EXE [-a ARGS], --processStartAndWait EXE [-a ARGS] ;
  --uninstall [-s]        crochet --squirrel-uninstall des programmes qui le
                          gèrent, puis désinstallation par Winlux (raccourcis,
                          fichiers, inscription), comme Squirrel (#1821).

Le paquet téléchargé est une donnée non fiable : il reste dans packages/,
il est vérifié (taille, SHA-1 publié dans RELEASES), et son extraction refuse
tout chemin qui sortirait du dossier de destination."""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

CONFIRM = os.environ.get("WINLUX_SQUIRREL_CONFIRM", "/usr/lib/winlux/winlux-store-confirm")
WINLUX = os.environ.get("WINLUX_BIN", "/usr/bin/winlux")
MAX_PACKAGE = 2 << 30  # au-delà, le paquet est refusé
HOOK_TIMEOUT = 30

LINE = re.compile(r"^([0-9a-fA-F]{40})\s+(\S+)\s+(\d+)\s*$")
PACKAGE = re.compile(r"^(?P<id>.+?)-(?P<ver>\d+(?:\.\d+)*(?:-(?!(?:full|delta)\.nupkg$)[0-9A-Za-z.]+)?)"
                     r"(?:-(?P<kind>full|delta))?\.nupkg$", re.IGNORECASE)


def interface_lang():
    """Langue de l'interface : WINLUX_LANG, /etc/winlux/locale.conf, LANG."""
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path("/etc/winlux/locale.conf").read_text().splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


T = {
    "fr": {
        "title": "Mettre à jour {product} ?",
        "text": ("{product} demande à télécharger sa nouvelle version.\n\n"
                 "Produit : {product}\nVersion : {current} → {version}\nÉditeur : {publisher}\n"
                 "Source : {url}\nTaille : {size}\nEmpreinte SHA-1 publiée : {sha1}\n\n"
                 "Si vous refusez, Winlux ne redemandera pas pour cette version."),
        "ok": "Mettre à jour", "cancel": "Annuler", "unknown": "inconnu",
    },
    "en": {
        "title": "Update {product}?",
        "text": ("{product} asks to download its new version.\n\n"
                 "Product: {product}\nVersion: {current} → {version}\nPublisher: {publisher}\n"
                 "Source: {url}\nSize: {size}\nPublished SHA-1: {sha1}\n\n"
                 "If you decline, Winlux will not ask again for this version."),
        "ok": "Update", "cancel": "Cancel", "unknown": "unknown",
    },
    "de": {
        "title": "{product} aktualisieren?",
        "text": ("{product} möchte seine neue Version herunterladen.\n\n"
                 "Produkt: {product}\nVersion: {current} → {version}\nHerausgeber: {publisher}\n"
                 "Quelle: {url}\nGröße: {size}\nVeröffentlichter SHA-1: {sha1}\n\n"
                 "Wenn Sie ablehnen, fragt Winlux für diese Version nicht erneut."),
        "ok": "Aktualisieren", "cancel": "Abbrechen", "unknown": "unbekannt",
    },
    "es": {
        "title": "¿Actualizar {product}?",
        "text": ("{product} pide descargar su nueva versión.\n\n"
                 "Producto: {product}\nVersión: {current} → {version}\nEditor: {publisher}\n"
                 "Origen: {url}\nTamaño: {size}\nSHA-1 publicado: {sha1}\n\n"
                 "Si lo rechaza, Winlux no volverá a preguntar por esta versión."),
        "ok": "Actualizar", "cancel": "Cancelar", "unknown": "desconocido",
    },
}


class Fail(Exception):
    """Erreur rapportée à l'appli par un code de sortie non nul."""


def log(root, message):
    line = f"winlux-squirrel-update: {message}"
    print(line, file=sys.stderr)
    try:
        with open(root / "SquirrelSetup.log", "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except OSError:
        pass


def version_key(text):
    """Ordre des versions : numéros d'abord, une préversion avant la finale."""
    main, _, pre = text.partition("-")
    nums = tuple(int(n) if n.isdigit() else 0 for n in main.split("."))
    nums += (0,) * (4 - len(nums))
    return nums, (pre == "", pre)


def parse_releases(text):
    """Entrées d'un fichier RELEASES : (sha1, nom, taille, id, version, delta)."""
    entries = []
    for raw in text.lstrip("﻿").splitlines():
        raw = raw.split("#", 1)[0].strip()
        m = LINE.match(raw)
        if not m:
            continue
        sha1, ref, size = m.group(1).lower(), m.group(2), int(m.group(3))
        name = urllib.parse.unquote(urllib.parse.urlsplit(ref).path.rsplit("/", 1)[-1])
        p = PACKAGE.match(name)
        if not p:
            continue
        entries.append({
            "sha1": sha1, "ref": ref, "filename": name, "size": size,
            "id": p.group("id"), "version": p.group("ver"),
            "isDelta": (p.group("kind") or "").lower() == "delta",
        })
    return entries


def installed_versions(root):
    out = []
    for d in root.glob("app-*"):
        if d.is_dir() and re.fullmatch(r"\d+(\.\d+)*(-[0-9A-Za-z.]+)?", d.name[4:]):
            out.append(d.name[4:])
    return sorted(out, key=version_key)


def local_state(root):
    """(id du paquet, version installée la plus récente)."""
    entries = []
    try:
        entries = parse_releases((root / "packages" / "RELEASES").read_text(encoding="utf-8"))
    except OSError:
        pass
    versions = installed_versions(root) + [e["version"] for e in entries if not e["isDelta"]]
    current = max(versions, key=version_key) if versions else None
    package_id = entries[0]["id"] if entries else root.name
    return package_id, current


def open_url(url):
    scheme = urllib.parse.urlsplit(url).scheme.lower()
    if scheme not in ("https", "file"):
        raise Fail(f"source refusée (seuls https et file sont acceptés) : {url}")
    return urllib.request.urlopen(url, timeout=60)


def base_url(url):
    return url if url.endswith("/") else url + "/"


def fetch_releases(url, package_id, current):
    target = urllib.parse.urljoin(base_url(url), "RELEASES")
    if urllib.parse.urlsplit(target).scheme.lower() == "https":
        query = {"id": package_id, "arch": "amd64"}
        if current:
            query["localVersion"] = current
        target += "?" + urllib.parse.urlencode(query)
    with open_url(target) as r:
        data = r.read(4 << 20)
    return parse_releases(data.decode("utf-8", "replace"))


def refused_marker(root, version):
    return root / "packages" / f".winlux-refus-{version}"


def check(root, url):
    package_id, current = local_state(root)
    remote = [e for e in fetch_releases(url, package_id, current)
              if e["id"].lower() == package_id.lower() or package_id == root.name]
    fulls = [e for e in remote if not e["isDelta"]]
    newer = [e for e in fulls if current is None or version_key(e["version"]) > version_key(current)]
    newer.sort(key=lambda e: version_key(e["version"]))
    if newer and refused_marker(root, newer[-1]["version"]).exists():
        log(root, f"version {newer[-1]['version']} refusée par l'utilisateur, pas proposée")
        newer = []
    future = newer[-1]["version"] if newer else current
    info = {
        "currentVersion": current,
        "futureVersion": future,
        "releasesToApply": [
            {"version": e["version"], "releaseNotes": "", "filename": e["filename"],
             "sha1": e["sha1"].upper(), "size": e["size"], "isDelta": False}
            for e in newer[-1:]
        ],
    }
    return info, newer[-1] if newer else None, url, package_id


def nuspec(path):
    """Champs <title> et <authors> du .nuspec d'un paquet."""
    try:
        with zipfile.ZipFile(path) as z:
            name = next((n for n in z.namelist() if n.endswith(".nuspec") and "/" not in n), None)
            text = z.read(name).decode("utf-8", "replace") if name else ""
    except (OSError, zipfile.BadZipFile, KeyError):
        return {}
    out = {}
    for field in ("title", "authors", "id"):
        m = re.search(rf"<{field}>([^<]*)</{field}>", text)
        if m and m.group(1).strip():
            out[field] = m.group(1).strip()
    return out


def human_size(n):
    return f"{n / (1 << 20):.1f} Mo" if n >= 1 << 20 else f"{n} o"


def confirm(root, entry, current, package_id, source):
    lang = T[interface_lang()]
    meta = {}
    if current:
        for p in (root / "packages").glob(f"*-{current}-full.nupkg"):
            meta = nuspec(p)
            break
    product = meta.get("title") or package_id
    values = {
        "product": product, "current": current or lang["unknown"], "version": entry["version"],
        "publisher": meta.get("authors") or lang["unknown"], "url": source,
        "size": human_size(entry["size"]), "sha1": entry["sha1"].upper(),
    }
    try:
        answer = subprocess.run(
            [CONFIRM, "--title", lang["title"].format(**values), "--text", lang["text"].format(**values),
             "--ok", lang["ok"], "--cancel", lang["cancel"]], timeout=24 * 3600).returncode
    except (OSError, subprocess.TimeoutExpired):
        answer = 2
    if answer == 0:
        return True
    if answer == 1:
        try:
            refused_marker(root, entry["version"]).touch()
        except OSError:
            pass
    log(root, f"mise à jour {entry['version']} non confirmée (code {answer}), rien n'est téléchargé")
    return False


def download(root, url, entry, package_id, current):
    packages = root / "packages"
    packages.mkdir(exist_ok=True)
    final = packages / entry["filename"]
    if final.name != entry["filename"] or "/" in entry["filename"]:
        raise Fail(f"nom de paquet refusé : {entry['filename']}")
    if final.is_file() and final.stat().st_size == entry["size"]:
        if hashlib.sha1(final.read_bytes()).hexdigest() == entry["sha1"]:
            return final
    if entry["size"] > MAX_PACKAGE:
        raise Fail(f"paquet trop gros ({entry['size']} octets)")
    source = urllib.parse.urljoin(base_url(url), entry["ref"])
    if not confirm(root, entry, current, package_id, source):
        raise Fail("téléchargement refusé")
    partial = packages / (entry["filename"] + ".partiel")
    sha1, sha256, total = hashlib.sha1(), hashlib.sha256(), 0
    with open_url(source) as r, open(partial, "wb") as out:
        while chunk := r.read(1 << 20):
            total += len(chunk)
            if total > entry["size"]:
                break
            sha1.update(chunk)
            sha256.update(chunk)
            out.write(chunk)
    if total != entry["size"] or sha1.hexdigest() != entry["sha1"]:
        partial.unlink(missing_ok=True)
        raise Fail(f"paquet {entry['filename']} non conforme (taille {total}, SHA-1 {sha1.hexdigest()})")
    partial.replace(final)
    log(root, f"paquet {entry['filename']} vérifié, SHA-256 {sha256.hexdigest()}")
    return final


def extract(package, dest):
    """Copie lib/net45/ du paquet dans dest, sans jamais en sortir."""
    dest.mkdir(parents=True)
    base = dest.resolve()
    with zipfile.ZipFile(package) as z:
        for info in z.infolist():
            name = info.filename.replace("\\", "/")
            if not name.lower().startswith("lib/net45/") or name.endswith("/"):
                continue
            rel = name[len("lib/net45/"):]
            target = (base / rel).resolve()
            if base not in target.parents:
                raise Fail(f"chemin refusé dans le paquet : {info.filename}")
            if (info.external_attr >> 16) & 0o170000 == 0o120000:
                raise Fail(f"lien symbolique refusé dans le paquet : {info.filename}")
            target.parent.mkdir(parents=True, exist_ok=True)
            with z.open(info) as src, open(target, "wb") as out:
                shutil.copyfileobj(src, out)


def squirrel_aware(exe):
    try:
        return "SquirrelAwareVersion".encode("utf-16-le") in exe.read_bytes()
    except OSError:
        return False


def install(root, package, entry):
    dest = root / f"app-{entry['version']}"
    if dest.is_dir():
        return dest
    staging = root / f"app-{entry['version']}.partiel"
    if staging.exists():
        shutil.rmtree(staging)
    try:
        extract(package, staging)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    staging.rename(dest)
    releases = root / "packages" / "RELEASES"
    line = f"{entry['sha1'].upper()} {entry['filename']} {entry['size']}"
    try:
        old = releases.read_text(encoding="utf-8").splitlines()
    except OSError:
        old = []
    if line not in old:
        releases.write_text("\n".join([*old, line]) + "\n", encoding="utf-8")
    for exe in sorted(dest.glob("*.exe")):
        if squirrel_aware(exe):
            log(root, f"crochet {exe.name} --squirrel-updated {entry['version']}")
            try:
                subprocess.run([WINLUX, str(exe), "--squirrel-updated", entry["version"]],
                               cwd=dest, timeout=HOOK_TIMEOUT, stdout=subprocess.DEVNULL)
            except (OSError, subprocess.TimeoutExpired) as e:
                log(root, f"crochet {exe.name} : {e}")
    return dest


def uninstall(root):
    """Update.exe --uninstall : Squirrel prévient l'appli (--squirrel-uninstall),
    puis retire tout. Winlux retire ce qu'il a installé par sa propre
    désinstallation, retrouvée par le nom du dossier (l'Id du paquet)."""
    versions = installed_versions(root)
    if versions:
        dest = root / f"app-{versions[-1]}"
        for exe in sorted(dest.glob("*.exe")):
            if squirrel_aware(exe):
                log(root, f"crochet {exe.name} --squirrel-uninstall")
                try:
                    subprocess.run([WINLUX, str(exe), "--squirrel-uninstall", versions[-1]],
                                   cwd=dest, timeout=HOOK_TIMEOUT, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.DEVNULL)
                except (OSError, subprocess.TimeoutExpired) as e:
                    log(root, f"crochet {exe.name} : {e}")
    listing = subprocess.run([WINLUX, "--setup", "--programs"], capture_output=True,
                             text=True, errors="replace", timeout=120).stdout
    wanted = root.name.casefold()
    for line in listing.splitlines():
        if not line.startswith("app="):
            continue
        key = line[4:].split("\t", 1)[0]
        if key.rsplit("\\", 1)[-1].casefold() == wanted:
            rc = subprocess.run([WINLUX, "--setup", "--remove", key], timeout=600,
                                stdout=subprocess.DEVNULL).returncode
            log(root, f"désinstallation {key} : code {rc}")
            # Seulement un vrai dossier Squirrel (app-* présents au départ).
            if rc == 0 and versions:
                # Ce que l'appli a écrit elle-même (installer.db, Discord.exe,
                # anciennes versions) : Squirrel retire tout le dossier.
                shutil.rmtree(root, ignore_errors=True)
            return rc
    raise Fail(f"{root.name} : aucune installation inscrite à désinstaller")


def process_start(root, exe_name, args, wait):
    name = Path(exe_name.replace("\\", "/")).name
    for version in reversed(installed_versions(root)):
        exe = root / f"app-{version}" / name
        if exe.is_file():
            cmd = [WINLUX, str(exe), *args]
            if wait:
                return subprocess.run(cmd, cwd=exe.parent).returncode
            subprocess.Popen(cmd, cwd=exe.parent, start_new_session=True,
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL)
            return 0
    raise Fail(f"{name} introuvable dans les dossiers app-*")


def parse_args(argv):
    """`--cmd valeur` ou `--cmd=valeur`, plus `-a` / `--process-start-args`."""
    command, value, extra = None, None, []
    i = 0
    while i < len(argv):
        arg = argv[i]
        key, eq, inline = arg.partition("=")
        if key in ("-a", "--process-start-args"):
            extra_value = inline if eq else (argv[i + 1] if i + 1 < len(argv) else "")
            extra = extra_value.split() if extra_value else []
            i += 1 if eq else 2
            continue
        if key.startswith("--") and command is None:
            command = key[2:]
            if eq:
                value = inline
            elif i + 1 < len(argv) and not argv[i + 1].startswith("-"):
                value = argv[i + 1]
                i += 1
        i += 1
    return command, value, extra


def main(argv):
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    root = Path(argv[1])
    command, value, extra = parse_args(argv[2:])
    try:
        if command == "checkForUpdate" and value:
            info, _, _, _ = check(root, value)
            print(json.dumps(info))
            return 0
        if command in ("download", "update") and value:
            info, entry, url, package_id = check(root, value)
            if entry is None:
                print(json.dumps(info))
                return 0
            package = download(root, url, entry, package_id, info["currentVersion"])
            if command == "update":
                install(root, package, entry)
            print(json.dumps(info))
            return 0
        if command in ("processStart", "processStartAndWait") and value:
            return process_start(root, value, extra, command == "processStartAndWait")
        if command == "uninstall":
            return uninstall(root)
        raise Fail(f"commande Squirrel non prise en charge : {' '.join(argv[2:])}")
    except Fail as e:
        log(root, str(e))
        return 1
    except (OSError, ValueError, zipfile.BadZipFile) as e:
        log(root, f"échec : {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
