#!/usr/bin/python3
"""Boutiques de logiciels proposées par Winlux (docs/BOUTIQUES.md).

Aucune boutique tierce n'est fournie dans l'image ni présélectionnée. Celles
que l'utilisateur choisit (installateur, premier démarrage, Paramètres >
Applications, Bienvenue) sont ajoutées au Store Winlux : le nécessaire est
téléchargé chez l'éditeur et vérifié, puis un lanceur apparaît dans Toutes les
applications et s'épingle au menu Démarrer. Sans réseau, la boutique reste
« en attente » et l'init la reprend au démarrage suivant.

  winlux-store-catalogs --list               id|état|plateforme|nom|résumé|site
                                             (état : on, pending, off)
  winlux-store-catalogs --set [id...]        exactement ces boutiques (root)
  winlux-store-catalogs --add|--remove id    une boutique de plus ou de moins (root)
  winlux-store-catalogs --resume             reprend les boutiques en attente (root)
  winlux-store-catalogs --check-key id       télécharge la clé de l'éditeur et
                                             compare son empreinte (sans root)

Codes de sortie : 0 fait, 75 reporté (réseau absent : reprise au prochain
démarrage), 1 refusé (empreinte différente, outil absent), 2 usage.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request

ROOT = os.environ.get("WINLUX_CONTROL_ROOT", "")
DEFS = Path(os.environ.get("WINLUX_STORE_CATALOGS_DEFS", ROOT + "/usr/share/winlux/store-catalogs.json"))
CHOSEN = Path(ROOT + "/etc/winlux/store-catalogs")
PENDING = Path(ROOT + "/etc/winlux/store-catalogs-pending")
LAUNCHERS = Path(ROOT + "/usr/local/share/applications")
REPOS = Path(ROOT + "/etc/winlux/store-repos")
LOG = Path(ROOT + "/var/log/winlux-store-catalogs.log")
OFFLINE = os.environ.get("WINLUX_STORE_OFFLINE") == "1"
LANGS = ("fr", "en", "es", "de")
DEFERRED = 75
TOOL = 3


def interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path(ROOT + "/etc/winlux/locale.conf").read_text().splitlines():
                if line.startswith("LANG="):
                    code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in LANGS else "fr"


LANG = interface_lang()
T = {
    "fr": {"added": "{name} est ajouté au Store.", "removed": "{name} est retiré du Store.",
           "deferred": "{name} : pas de réseau, ajout reporté au prochain démarrage.",
           "key": "{name} : la clé de signature reçue ne correspond pas à celle attendue, boutique refusée.",
           "tool": "{name} : impossible d'installer Flatpak ou d'ajouter la boutique (voir /var/log/winlux-store-catalogs.log).",
           "unknown": "Boutique inconnue : {id}", "root": "Droits administrateur requis."},
    "en": {"added": "{name} is added to the Store.", "removed": "{name} is removed from the Store.",
           "deferred": "{name}: no network, will be added at next startup.",
           "key": "{name}: the signing key received does not match the expected one, store refused.",
           "tool": "{name}: unable to install Flatpak or add the store (see /var/log/winlux-store-catalogs.log).",
           "unknown": "Unknown store: {id}", "root": "Administrator rights are required."},
    "es": {"added": "{name} se ha añadido a la Tienda.", "removed": "{name} se ha quitado de la Tienda.",
           "deferred": "{name}: sin red, se añadirá en el próximo arranque.",
           "key": "{name}: la clave de firma recibida no coincide con la esperada, tienda rechazada.",
           "tool": "{name}: no se pudo instalar Flatpak ni añadir la tienda (consulte /var/log/winlux-store-catalogs.log).",
           "unknown": "Tienda desconocida: {id}", "root": "Se necesitan derechos de administrador."},
    "de": {"added": "{name} wurde dem Store hinzugefügt.", "removed": "{name} wurde aus dem Store entfernt.",
           "deferred": "{name}: kein Netzwerk, wird beim nächsten Start hinzugefügt.",
           "key": "{name}: der empfangene Signaturschlüssel stimmt nicht mit dem erwarteten überein, Store abgelehnt.",
           "tool": "{name}: Flatpak konnte nicht installiert oder der Store nicht hinzugefügt werden (siehe /var/log/winlux-store-catalogs.log).",
           "unknown": "Unbekannter Store: {id}", "root": "Administratorrechte erforderlich."},
}
# Lanceur de chaque boutique : nom et description dans les quatre langues.
COMMENT = {"fr": "Logiciels de la boutique {name}, dans le Store Winlux",
           "en": "{name} software, in the Winlux Store",
           "es": "Programas de la tienda {name}, en la Tienda Winlux",
           "de": "Programme aus dem {name}-Store, im Winlux Store"}


def tr(key, **values):
    return (T[LANG].get(key) or T["fr"][key]).format(**values)


def say(text):
    print(text)
    try:
        LOG.parent.mkdir(parents=True, exist_ok=True)
        with LOG.open("a", encoding="utf-8") as log:
            log.write(text + "\n")
    except OSError:
        pass


def catalogs():
    try:
        data = json.loads(DEFS.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return []
    found = [c for c in data.get("catalogs", []) if isinstance(c, dict) and re.fullmatch(r"[a-z0-9-]+", str(c.get("id", "")))]
    return sorted(found, key=lambda c: c["name"].casefold())  # ordre alphabétique : aucune mise en avant


def by_id():
    return {c["id"]: c for c in catalogs()}


def read_ids(path):
    known = by_id()
    try:
        ids = [line.strip() for line in path.read_text().splitlines()]
    except OSError:
        return []
    return [i for n, i in enumerate(ids) if i in known and i not in ids[:n]]


def write_ids(path, ids):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f"{path.name}.{os.getpid()}")
    temporary.write_text("".join(i + "\n" for i in ids))
    temporary.chmod(0o644)
    os.replace(temporary, path)


def chosen():
    return read_ids(CHOSEN)


# ── Empreinte OpenPGP de la clé publiée par l'éditeur ───────────────────────

def openpgp_fingerprint(blob):
    """Empreinte de la première clé publique d'un trousseau binaire OpenPGP
    (v4 : SHA-1, v6 : SHA-256), en hexadécimal majuscule ; None si illisible."""
    if len(blob) < 3:
        return None
    tag = blob[0]
    if tag & 0x80 == 0:
        return None
    if tag & 0x40:  # format nouveau
        if (tag & 0x3F) != 6:
            return None
        first = blob[1]
        if first < 192:
            length, start = first, 2
        elif first < 224:
            length, start = ((first - 192) << 8) + blob[2] + 192, 3
        elif first == 255:
            length, start = int.from_bytes(blob[2:6], "big"), 6
        else:
            return None
    else:  # ancien format
        if ((tag >> 2) & 0x0F) != 6:
            return None
        size = {0: 1, 1: 2, 2: 4}.get(tag & 3)
        if size is None:
            return None
        length, start = int.from_bytes(blob[1:1 + size], "big"), 1 + size
    body = blob[start:start + length]
    if len(body) != length or not body:
        return None
    if body[0] == 4:
        return hashlib.sha1(b"\x99" + length.to_bytes(2, "big") + body).hexdigest().upper()
    if body[0] == 6:
        return hashlib.sha256(b"\x9b" + length.to_bytes(4, "big") + body).hexdigest().upper()
    return None


def flatpakrepo_key(text):
    for line in text.splitlines():
        if line.startswith("GPGKey="):
            try:
                return base64.b64decode(line[7:].strip(), validate=True)
            except ValueError:
                return None
    return None


def fetch(url, timeout=30):
    if OFFLINE:
        return None
    if url.startswith("file:"):
        try:
            return Path(url[5:]).read_bytes()
        except OSError:
            return None
    if not url.startswith("https://"):
        return None
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "Winlux-Store/1.0"})
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.read(1 << 20)
    except Exception:
        return None


def verified_repo(catalog):
    """(chemin du .flatpakrepo vérifié, None) ou (None, code)."""
    data = fetch(os.environ.get("WINLUX_FLATHUB_REPO", catalog["repo"]))
    if data is None:
        return None, DEFERRED
    try:
        text = data.decode("utf-8")
    except UnicodeError:
        return None, 1
    key = flatpakrepo_key(text)
    expected = re.sub(r"\s", "", catalog.get("fingerprint", "")).upper()
    if not key or not expected or openpgp_fingerprint(key) != expected:
        return None, 1
    handle = tempfile.NamedTemporaryFile("w", suffix=".flatpakrepo", delete=False, encoding="utf-8")
    with handle:
        handle.write(text)
    return Path(handle.name), None


# ── Ajout et retrait ────────────────────────────────────────────────────────

def run(command, timeout=900):
    try:
        return subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=timeout).returncode
    except (OSError, subprocess.SubprocessError):
        return 127


def network_up():
    if OFFLINE:
        return False
    try:
        return any(line.split()[1] == "00000000" for line in Path(ROOT + "/proc/net/route").read_text().splitlines()[1:]
                   if len(line.split()) > 1)
    except OSError:
        return True  # sans /proc (tests) : le téléchargement dira


def repo_path(catalog):
    return REPOS / f"{catalog['id']}.flatpakrepo"


def repo_valid(path, catalog):
    try:
        key = flatpakrepo_key(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError):
        return False
    return bool(key) and openpgp_fingerprint(key) == re.sub(r"\s", "", catalog.get("fingerprint", "")).upper()


def enable_flatpak(catalog):
    if not shutil.which("flatpak"):
        if not network_up():
            return DEFERRED
        # Flatpak vient des dépôts du système (Ubuntu), pas de l'image.
        env = {**os.environ, "DEBIAN_FRONTEND": "noninteractive"}
        try:
            subprocess.run(["apt-get", "update"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=900, env=env)
            code = subprocess.run(["apt-get", "install", "-y", "--no-install-recommends", "flatpak"],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1800, env=env).returncode
        except (OSError, subprocess.SubprocessError):
            code = 127
        if code != 0 or not shutil.which("flatpak"):
            return DEFERRED if not network_up() else TOOL
    target = repo_path(catalog)
    if repo_valid(target, catalog):
        return 0
    path, code = verified_repo(catalog)
    if path is None:
        return code
    # Dépôt vérifié gardé pour tous les comptes : chacun le déclare pour lui
    # (flatpak --user, sans réseau) à l'ouverture du Store.
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(path), target)
    target.chmod(0o644)
    return 0


def disable_flatpak(catalog):
    # Les applications déjà installées restent ; la boutique n'est plus proposée
    # (le Store masque le dépôt) et aucun nouveau compte ne la reçoit.
    repo_path(catalog).unlink(missing_ok=True)


def launcher_path(catalog):
    return LAUNCHERS / f"winlux-store-{catalog['id']}.desktop"


def write_launcher(catalog):
    name = catalog["name"]
    lines = ["[Desktop Entry]", "Type=Application", f"Name={name}"]
    lines.append("Comment=" + COMMENT["fr"].format(name=name))
    for lang in ("en", "es", "de"):
        lines.append(f"Comment[{lang}]=" + COMMENT[lang].format(name=name))
    lines += [f"Exec=/usr/bin/winlux-shell-app store {catalog['source']} {name.replace(' ', '_')}",
              "Icon=system-software-install", "Terminal=false", "Categories=System;PackageManager;",
              "Keywords=Boutique;Store;Tienda;Logiciels;Software;Programas;Programme;",
              # Épinglé une fois au menu Démarrer de chaque compte (winlux-start-menu).
              "X-Winlux-Pin=true", f"X-Winlux-Store-Catalog={catalog['id']}"]
    path = launcher_path(catalog)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text("\n".join(lines) + "\n", encoding="utf-8")
    temporary.chmod(0o644)
    os.replace(temporary, path)


def enable(catalog):
    code = enable_flatpak(catalog) if catalog.get("kind") == "flatpak" else 0
    # Catalogue Windows : rien à installer, le Store télécharge les manifestes
    # (gardés en cache) et vérifie l'empreinte de chaque installateur.
    pending = [i for i in read_ids(PENDING) if i != catalog["id"]]
    if code == 0:
        write_launcher(catalog)
        say(tr("added", name=catalog["name"]))
    else:
        launcher_path(catalog).unlink(missing_ok=True)
        pending.append(catalog["id"])
        say(tr({DEFERRED: "deferred", TOOL: "tool"}.get(code, "key"), name=catalog["name"]))
    write_ids(PENDING, pending)
    return DEFERRED if code == DEFERRED else 0 if code == 0 else 1


def disable(catalog):
    if catalog.get("kind") == "flatpak":
        disable_flatpak(catalog)
    launcher_path(catalog).unlink(missing_ok=True)
    write_ids(PENDING, [i for i in read_ids(PENDING) if i != catalog["id"]])
    say(tr("removed", name=catalog["name"]))


def apply(ids):
    known = by_id()
    previous = set(chosen())
    write_ids(CHOSEN, [i for i in known if i in ids])
    worst = 0
    for ident in known:
        if ident in ids and (ident not in previous or ident in read_ids(PENDING) or not launcher_path(known[ident]).exists()):
            code = enable(known[ident])
            worst = code if code and worst != 1 else worst
        elif ident not in ids and ident in previous:
            disable(known[ident])
    refresh_menus()
    return worst


def refresh_menus():
    if shutil.which("update-desktop-database") and LAUNCHERS.is_dir():
        run(["update-desktop-database", str(LAUNCHERS)], 60)


def state(ident, current, pending):
    return "pending" if ident in pending else "on" if ident in current else "off"


def main(argv):
    args = argv[1:]
    known = by_id()
    if args[:1] == ["--list"]:
        current, pending = set(chosen()), set(read_ids(PENDING))
        for c in catalogs():
            summary = c.get("summary", {})
            text = summary.get(LANG) or summary.get("fr", "")
            print("|".join(str(v).replace("|", "/") for v in (c["id"], state(c["id"], current, pending),
                                                                c.get("platform", ""), c["name"], text, c.get("site", ""))))
        return 0
    if args[:1] == ["--check-key"] and len(args) == 2:
        catalog = known.get(args[1])
        if not catalog or catalog.get("kind") != "flatpak":
            print(tr("unknown", id=args[1]), file=sys.stderr)
            return 2
        data = fetch(catalog["repo"])
        if data is None:
            return DEFERRED
        key = flatpakrepo_key(data.decode("utf-8", "replace"))
        print(openpgp_fingerprint(key) if key else "?")
        return 0 if key and openpgp_fingerprint(key) == re.sub(r"\s", "", catalog.get("fingerprint", "")).upper() else 1
    unknown = [i for i in args[1:] if i not in known]
    if args[:1] in (["--set"], ["--add"], ["--remove"], ["--resume"]):
        if unknown:
            print(tr("unknown", id=unknown[0]), file=sys.stderr)
            return 2
        if os.geteuid() != 0 and not ROOT:
            print(tr("root"), file=sys.stderr)
            return 1
    if args[:1] == ["--set"]:
        return apply(set(args[1:]))
    if args[:1] == ["--add"] and len(args) == 2:
        return apply(set(chosen()) | {args[1]})
    if args[:1] == ["--remove"] and len(args) == 2:
        return apply(set(chosen()) - {args[1]})
    if args == ["--resume"]:
        worst = 0
        for ident in read_ids(PENDING):
            if ident in chosen():
                code = enable(known[ident])
                worst = code if code and worst != 1 else worst
            else:
                write_ids(PENDING, [i for i in read_ids(PENDING) if i != ident])
        refresh_menus()
        return worst
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
