#!/usr/bin/python3
"""Jeux : la bibliothèque des jeux Windows (.exe) installés sur Winlux.

Rassemble sans rien exécuter :
- les jeux GOG (fichiers goggame-*.info laissés par leurs installateurs) ;
- les jeux itch.io (dossiers contenant .itch/receipt.json.gz) ;
- les programmes Windows déjà inscrits (registre Uninstall, raccourcis .lnk,
  programmes lancés une fois) qui importent Direct3D, OpenGL, Vulkan ou
  XInput, le même critère que le mode jeu du runtime ;
- les dossiers de jeux usuels (C:\\Games, C:\\Jeux, C:\\GOG Games…) et ceux
  ajoutés dans Paramètres › Jeux ;
- les .exe ajoutés à la main.

Le lancement passe par winlux-open : choix du GPU, mode jeu et overlay de
performances sont appliqués par le runtime comme pour un double-clic. Le temps
de jeu vient des séances écrites par le runtime (runtime/src/playtime.rs).

Réglages : ~/.config/winlux/jeux.conf (lignes CLÉ=valeur, jamais exécutées)
  PLAYTIME=0|1   AUTOSCAN=0|1   FOLDER=/chemin   HIDDEN=/jeu.exe   ADDED=/jeu.exe
"""
import fcntl
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import time
import unicodedata


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}

# Texte français -> (anglais, espagnol, allemand).
_TR = {
    'Jeux': ('Games', 'Juegos', 'Spiele'),
    'Bibliothèque': ('Library', 'Biblioteca', 'Bibliothek'),
    'Rechercher un jeu': ('Search for a game', 'Buscar un juego', 'Spiel suchen'),
    'Récents': ('Recent', 'Recientes', 'Zuletzt gespielt'),
    'Nom': ('Name', 'Nombre', 'Name'),
    'Temps de jeu': ('Play time', 'Tiempo de juego', 'Spielzeit'),
    'Ajouter un jeu': ('Add a game', 'Agregar un juego', 'Spiel hinzufügen'),
    'Rechercher les jeux': ('Look for games', 'Buscar juegos', 'Nach Spielen suchen'),
    'Réglages': ('Settings', 'Configuración', 'Einstellungen'),
    'Jouer': ('Play', 'Jugar', 'Spielen'),
    'En cours': ('Running', 'En curso', 'Läuft'),
    'Jamais lancé': ('Never played', 'Nunca jugado', 'Nie gespielt'),
    'Joué le {date}': ('Played on {date}', 'Jugado el {date}', 'Gespielt am {date}'),
    'moins d’une minute': ('less than a minute', 'menos de un minuto', 'weniger als eine Minute'),
    '{m} min': ('{m} min', '{m} min', '{m} Min.'),
    '{h} h {m:02d} min': ('{h} h {m:02d} min', '{h} h {m:02d} min', '{h} Std. {m:02d} Min.'),
    'Épingler au menu Démarrer': ('Pin to Start', 'Anclar a Inicio', 'An „Start“ anheften'),
    'Créer un raccourci sur le bureau': ('Create a desktop shortcut', 'Crear un acceso directo en el escritorio', 'Desktopverknüpfung erstellen'),
    'Ouvrir l’emplacement': ('Open file location', 'Abrir la ubicación del archivo', 'Dateipfad öffnen'),
    'Retirer de la bibliothèque': ('Remove from library', 'Quitar de la biblioteca', 'Aus der Bibliothek entfernen'),
    'Recherche des jeux installés…': ('Looking for installed games…', 'Buscando juegos instalados…', 'Installierte Spiele werden gesucht…'),
    '{n} jeux': ('{n} games', '{n} juegos', '{n} Spiele'),
    '1 jeu': ('1 game', '1 juego', '1 Spiel'),
    'Aucun jeu pour l’instant': ('No games yet', 'Todavía no hay juegos', 'Noch keine Spiele'),
    'Installez un jeu Windows (.exe), puis revenez ici. Un jeu déjà présent ailleurs s’ajoute avec « Ajouter un jeu ».': ('Install a Windows game (.exe), then come back here. A game stored elsewhere can be added with “Add a game”.', 'Instale un juego de Windows (.exe) y vuelva aquí. Un juego guardado en otro lugar se agrega con «Agregar un juego».', 'Installieren Sie ein Windows-Spiel (.exe) und kehren Sie dann hierher zurück. Ein Spiel an einem anderen Ort fügen Sie mit „Spiel hinzufügen“ hinzu.'),
    'Choisir le programme du jeu': ('Choose the game program', 'Elegir el programa del juego', 'Spielprogramm auswählen'),
    'Programmes Windows': ('Windows programs', 'Programas de Windows', 'Windows-Programme'),
    'Annuler': ('Cancel', 'Cancelar', 'Abbrechen'),
    'Ajouter': ('Add', 'Agregar', 'Hinzufügen'),
    'Ce fichier n’est pas un programme Windows.': ('This file is not a Windows program.', 'Este archivo no es un programa de Windows.', 'Diese Datei ist kein Windows-Programm.'),
    'Épinglé au menu Démarrer.': ('Pinned to Start.', 'Anclado a Inicio.', 'An „Start“ angeheftet.'),
    'Raccourci créé sur le bureau.': ('Shortcut created on the desktop.', 'Acceso directo creado en el escritorio.', 'Verknüpfung auf dem Desktop erstellt.'),
    'Impossible de créer le raccourci.': ('Could not create the shortcut.', 'No se pudo crear el acceso directo.', 'Die Verknüpfung konnte nicht erstellt werden.'),
    'Programme 32 bits : pas encore pris en charge par Winlux.': ('32-bit program: not supported by Winlux yet.', 'Programa de 32 bits: Winlux aún no lo admite.', '32-Bit-Programm: von Winlux noch nicht unterstützt.'),
    'Le jeu est introuvable : il a peut-être été désinstallé ou déplacé.': ('The game cannot be found: it may have been uninstalled or moved.', 'No se encuentra el juego: puede que se haya desinstalado o movido.', 'Das Spiel wurde nicht gefunden: Es wurde vielleicht deinstalliert oder verschoben.'),
    'GOG': ('GOG', 'GOG', 'GOG'),
    'itch.io': ('itch.io', 'itch.io', 'itch.io'),
    'Installé': ('Installed', 'Instalado', 'Installiert'),
    'Dossier de jeux': ('Games folder', 'Carpeta de juegos', 'Spieleordner'),
    'Ajouté à la main': ('Added manually', 'Agregado manualmente', 'Manuell hinzugefügt'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def fold(text):
    """Recherche sans casse ni accents (même règle que le menu Démarrer)."""
    return "".join(c for c in unicodedata.normalize("NFKD", text) if not unicodedata.combining(c)).casefold()


# ── Emplacements ────────────────────────────────────────────────────────────

def home():
    return Path(os.environ.get("HOME") or Path.home())


def drive_c():
    """C:\\ vu par les programmes Windows, comme runtime/src/paths.rs."""
    value = os.environ.get("WINLUX_DRIVE_C") or os.environ.get("WINLUX_HOME")
    return Path(value) if value else home()


def config_path():
    base = os.environ.get("XDG_CONFIG_HOME") or str(home() / ".config")
    return Path(base) / "winlux/jeux.conf"


def data_dir():
    base = os.environ.get("XDG_DATA_HOME") or str(home() / ".local/share")
    return Path(base) / "winlux/jeux"


def cache_path():
    base = os.environ.get("XDG_CACHE_HOME") or str(home() / ".cache")
    return Path(base) / "winlux/jeux.json"


def applications_dir():
    base = os.environ.get("XDG_DATA_HOME") or str(home() / ".local/share")
    return Path(base) / "applications"


def desktop_dir():
    """Bureau XDG (~/Bureau en français), lu dans user-dirs.dirs sans shell."""
    dirs = Path(os.environ.get("XDG_CONFIG_HOME") or str(home() / ".config")) / "user-dirs.dirs"
    try:
        for line in dirs.read_text(errors="replace").splitlines():
            if line.startswith("XDG_DESKTOP_DIR="):
                value = line.split("=", 1)[1].strip().strip('"').replace("$HOME", str(home()))
                if value.startswith("/"): return Path(value)
    except OSError:
        pass
    for name in ("Bureau", "Desktop", "Escritorio", "Schreibtisch"):
        if (home() / name).is_dir(): return home() / name
    return home() / "Bureau"


# Dossiers de jeux usuels, relatifs à C:\ ; chaque sous-dossier est un jeu.
GAME_ROOTS = ("Games", "Jeux", "Juegos", "Spiele", "GOG Games", "Program Files/GOG Galaxy/Games",
              "Program Files (x86)/GOG Galaxy/Games", "AppData/Roaming/itch/apps", "Program Files",
              "Program Files (x86)", "AppData/Local/Programs")


# ── Réglages ────────────────────────────────────────────────────────────────

def load_config(path=None):
    config = {"PLAYTIME": "1", "AUTOSCAN": "1", "FOLDER": [], "HIDDEN": [], "ADDED": []}
    try:
        text = (path or config_path()).read_text(encoding="utf-8", errors="replace")[:256 * 1024]
    except OSError:
        return config
    for line in text.splitlines()[:4096]:
        key, sep, value = line.strip().partition("=")
        key, value = key.strip(), value.strip()
        if not sep or key.startswith("#"): continue
        if key in ("FOLDER", "HIDDEN", "ADDED"):
            if value.startswith("/") and value not in config[key]: config[key].append(value)
        elif key in ("PLAYTIME", "AUTOSCAN") and value in ("0", "1"):
            config[key] = value
    return config


def save_config(config, path=None):
    path = path or config_path()
    lines = ["# Réglages de l’application Jeux (Paramètres › Jeux)",
             f"PLAYTIME={config.get('PLAYTIME', '1')}", f"AUTOSCAN={config.get('AUTOSCAN', '1')}"]
    for key in ("FOLDER", "HIDDEN", "ADDED"):
        lines += [f"{key}={value}" for value in config.get(key, []) if "\n" not in value and "\r" not in value]
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + f".tmp.{os.getpid()}")
    temporary.write_text("\n".join(lines) + "\n", encoding="utf-8")
    temporary.replace(path)


def update_config(change, path=None):
    """Lecture, modification et écriture sous verrou (Paramètres et Jeux à la fois)."""
    path = path or config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.with_name(path.name + ".lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        config = load_config(path)
        change(config)
        save_config(config, path)
    return config


# ── Lecture des .exe (sans jamais les exécuter) ─────────────────────────────

def pe_info(path):
    """(64 bits ?, DLL importées en minuscules) ou None si ce n’est pas un .exe PE."""
    try:
        with open(path, "rb") as stream:
            head = stream.read(64)
            if len(head) < 64 or head[:2] != b"MZ": return None
            offset = struct.unpack_from("<I", head, 60)[0]
            if offset > 16 * 1024 * 1024: return None
            stream.seek(offset)
            header = stream.read(24)
            if len(header) < 24 or header[:4] != b"PE\0\0": return None
            sections, optional_size, flags = struct.unpack_from("<HxxxxxxxxxxxxHH", header, 6)
            if flags & 0x2000: return None  # DLL
            optional = stream.read(optional_size)
            if len(optional) < 2: return None
            magic = struct.unpack_from("<H", optional, 0)[0]
            wide = magic == 0x20B
            directories = 112 if wide else 96
            imports = []
            if len(optional) >= directories + 16:
                import_rva = struct.unpack_from("<I", optional, directories + 8)[0]
                table = stream.read(40 * min(sections, 96))
                spans = []
                for index in range(len(table) // 40):
                    vsize, vaddr, rsize, raw = struct.unpack_from("<IIII", table, index * 40 + 8)
                    spans.append((vaddr, max(vsize, rsize), raw))

                def file_offset(rva):
                    for vaddr, size, raw in spans:
                        if vaddr <= rva < vaddr + size: return rva - vaddr + raw
                    return None
                position = file_offset(import_rva) if import_rva else None
                for index in range(512 if position is not None else 0):
                    stream.seek(position + index * 20)
                    descriptor = stream.read(20)
                    if len(descriptor) < 20 or descriptor == b"\0" * 20: break
                    name_at = file_offset(struct.unpack_from("<I", descriptor, 12)[0])
                    if name_at is None: continue
                    stream.seek(name_at)
                    name = stream.read(128).split(b"\0", 1)[0]
                    imports.append(name.decode("ascii", "replace").lower())
            return wide, imports
    except (OSError, struct.error, ValueError):
        return None


def heavy(imports):
    """Même critère que cpu::is_heavy du runtime (mode jeu)."""
    return any(n.startswith("d3d") or n.startswith("dxgi") or n == "opengl32.dll" or n.startswith("vulkan-1")
               or n.startswith("xinput") for n in imports)


# Outils livrés avec les jeux, jamais le jeu lui-même.
NOT_GAMES = ("unins", "uninstall", "setup", "install", "crashreport", "crashhandler", "crashpad", "redist",
             "vcredist", "vc_redist", "dxsetup", "dxwebsetup", "directx", "dotnet", "prereq", "easyanticheat",
             "battleye", "be_service", "update", "patcher", "touchup", "cleanup", "helper", "report", "eac_",
             "ue4prereq", "unitycrash", "dowser", "cefprocess", "subprocess", "webhelper", "benchmark", "config")


def is_tool(path):
    stem = fold(Path(path).stem)
    return any(word in stem for word in NOT_GAMES)


def game_id(exe):
    return hashlib.sha256(str(exe).encode("utf-8", "surrogateescape")).hexdigest()[:16]


def pretty(name):
    name = name.replace("_", " ").strip()
    return name[:1].upper() + name[1:] if name else name


def best_exe(folder, limit=600, any_exe=False):
    """Le .exe principal d’un dossier de jeu : graphique, pas un outil, le moins profond puis le plus gros.

    any_exe : dossier déclaré comme dossier de jeux (C:\\Jeux…). Un jeu Unity
    ou chargeant Direct3D à la demande n’importe aucune DLL graphique dans son
    .exe : faute de mieux, le plus gros .exe hors outils est retenu."""
    found, fallback, seen = [], [], 0
    folder = Path(folder)
    stack = [(folder, 0)]
    while stack:
        current, depth = stack.pop()
        try: entries = sorted(current.iterdir(), key=lambda p: p.name)
        except OSError: continue
        for entry in entries:
            seen += 1
            if seen > limit: break
            try:
                if entry.is_symlink(): continue
                if entry.is_dir():
                    if depth < 3 and not entry.name.startswith(".") and fold(entry.name) not in (
                            "redist", "_commonredist", "directx", "vcredist", "support", "__installer"):
                        stack.append((entry, depth + 1))
                elif entry.suffix.lower() == ".exe" and not is_tool(entry):
                    info = pe_info(entry)
                    if info and heavy(info[1]):
                        found.append((depth, -entry.stat().st_size, str(entry), info[0]))
                    elif info and any_exe:
                        fallback.append((depth, -entry.stat().st_size, str(entry), info[0]))
            except OSError:
                continue
    found = found or fallback
    if not found: return None
    found.sort()
    return Path(found[0][2]), found[0][3]


def gog_games(folder):
    """goggame-*.info : nom et tâche principale écrits par l’installateur GOG."""
    games = []
    try: infos = sorted(Path(folder).glob("goggame-*.info"))[:4]
    except OSError: return games
    for info in infos:
        try:
            if info.stat().st_size > 1024 * 1024: continue
            value = json.loads(info.read_text(encoding="utf-8-sig", errors="replace"))
        except (OSError, ValueError):
            continue
        if not isinstance(value, dict): continue
        name = value.get("name") if isinstance(value.get("name"), str) else None
        tasks = [t for t in value.get("playTasks", []) if isinstance(t, dict)] if isinstance(value.get("playTasks"), list) else []
        tasks.sort(key=lambda t: not t.get("isPrimary"))
        for task in tasks:
            relative = task.get("path")
            if task.get("type", "FileTask") != "FileTask" or not isinstance(relative, str): continue
            exe = Path(folder) / relative.replace("\\", "/")
            if ".." in Path(relative.replace("\\", "/")).parts: continue
            if exe.suffix.lower() == ".exe" and exe.is_file():
                games.append((exe, name or pretty(exe.stem)))
                break
    return games


def itch_title(folder):
    receipt = Path(folder) / ".itch/receipt.json.gz"
    try:
        if not receipt.is_file() or receipt.stat().st_size > 4 * 1024 * 1024: return None
        with gzip.open(receipt, "rt", encoding="utf-8", errors="replace") as stream:
            value = json.loads(stream.read(4 * 1024 * 1024))
        title = value.get("game", {}).get("title")
        return title if isinstance(title, str) and title.strip() else None
    except (OSError, ValueError, EOFError, AttributeError):
        return None


def applications_module():
    """winlux-applications (registre, raccourcis .lnk, icônes des .exe), installé à côté."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve()
    for candidate in (Path("/usr/bin/winlux-applications"), here.parent.parent.parent / "scripts/winlux-applications.py"):
        if candidate.is_file():
            try:
                loader = SourceFileLoader("winlux_applications", str(candidate))
                spec = importlib.util.spec_from_loader("winlux_applications", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                return None
    return None


def scan(config=None):
    """Jeux trouvés : liste de dicts id, name, exe, source, wide."""
    config = config or load_config()
    hidden = set(config["HIDDEN"])
    games, known = [], set()

    def add(exe, name, source, wide=None):
        try: exe = Path(exe).resolve()
        except OSError: return
        key = str(exe)
        if key in known or key in hidden or not exe.is_file(): return
        if wide is None:
            info = pe_info(exe)
            if not info: return
            wide = info[0]
        known.add(key)
        games.append({"id": game_id(key), "name": (name or pretty(exe.stem))[:200], "exe": key,
                      "source": source, "wide": bool(wide)})

    for value in config["ADDED"]:
        add(value, None, "manual")
    roots = [drive_c() / root for root in GAME_ROOTS] + [Path(value) for value in config["FOLDER"]]
    for root in roots:
        try:
            if not root.is_dir(): continue
            children = sorted(child for child in root.iterdir() if child.is_dir() and not child.is_symlink())[:400]
        except OSError:
            continue
        program_files = root.name.lower().startswith("program files") or root.name == "Programs"
        for child in children:
            gog = gog_games(child)
            for exe, name in gog: add(exe, name, "gog")
            if gog: continue
            title = itch_title(child)
            found = best_exe(child, any_exe=not program_files)
            if not found: continue
            if title: add(found[0], title, "itch", found[1])
            elif not program_files or (child / "goggame.dll").is_file():
                add(found[0], pretty(child.name), "folder", found[1])
            else:
                add(found[0], pretty(child.name), "installed", found[1])
    apps = applications_module()
    if apps is not None:
        sources = []
        try:
            sources += [(Path(item["path"]), item.get("name")) for item in apps.records(home())
                        if isinstance(item, dict) and isinstance(item.get("path"), str)]
            sources += apps.installed_registry(home())
            sources += [(target, name) for target, name, _ in apps.shortcuts(home())]
        except Exception:
            pass
        for exe, name in sources[:4096]:
            if is_tool(exe): continue
            info = pe_info(exe)
            if info and heavy(info[1]): add(exe, name if isinstance(name, str) else None, "installed", info[0])
    games.sort(key=lambda game: fold(game["name"]))
    return games


def save_cache(games):
    path = cache_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + f".tmp.{os.getpid()}")
    temporary.write_text(json.dumps(games, ensure_ascii=False), encoding="utf-8")
    temporary.replace(path)


def load_cache():
    try:
        path = cache_path()
        if path.stat().st_size > 8 * 1024 * 1024: return []
        value = json.loads(path.read_text(encoding="utf-8"))
        return [g for g in value if isinstance(g, dict) and isinstance(g.get("exe"), str) and isinstance(g.get("name"), str)]
    except (OSError, ValueError):
        return []


# ── Temps de jeu (séances écrites par le runtime) ───────────────────────────

# Une séance non rafraîchie depuis 90 s (le runtime écrit toutes les 30 s) est finie.
FINISHED_AFTER = 90
MAX_SESSION = 7 * 24 * 3600


def parse_session(text):
    values = {}
    for line in text.splitlines()[:8]:
        key, sep, value = line.partition("=")
        if sep: values[key] = value
    try:
        exe, start, last = values["exe"], int(values["start"]), int(values["last"])
    except (KeyError, ValueError):
        return None
    if not exe.startswith("/") or last < start: return None
    return exe, start, min(last, start + MAX_SESSION)


def playtime(now=None):
    """{exe: {"seconds", "last", "running"}} ; les séances finies passent dans totals.tsv."""
    now = int(time.time()) if now is None else now
    base = data_dir()
    if not base.is_dir(): return {}
    finished, live = {}, []

    def add(table, exe, seconds, last):
        entry = table.setdefault(exe, {"seconds": 0, "last": 0, "running": False})
        entry["seconds"] += seconds
        entry["last"] = max(entry["last"], last)
        return entry
    with (base / "totals.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        totals = base / "totals.tsv"
        try: lines = totals.read_text(encoding="utf-8", errors="surrogateescape").splitlines()[:65536]
        except OSError: lines = []
        for line in lines:
            parts = line.split("\t", 2)
            if len(parts) == 3 and parts[0].isdigit() and parts[1].isdigit() and parts[2].startswith("/"):
                add(finished, parts[2], int(parts[0]), int(parts[1]))
        changed = False
        try: files = sorted((base / "sessions").glob("*.session"))[:4096]
        except OSError: files = []
        for path in files:
            try: parsed = parse_session(path.read_text(encoding="utf-8", errors="surrogateescape")[:8192])
            except OSError: continue
            # « <pid>-<début>.session » : un jeu dont le processus est fini
            # n'est plus « En cours » (le bouton Jouer restait grisé jusqu'au
            # délai de garde, même après avoir quitté le jeu).
            pid = path.name.split("-", 1)[0]
            alive = not pid.isdigit() or Path("/proc", pid).exists()
            if parsed is not None and alive and now - parsed[2] <= FINISHED_AFTER:
                live.append(parsed)
                continue
            if parsed is not None and "\t" not in parsed[0]:
                add(finished, parsed[0], parsed[2] - parsed[1], parsed[2])
            path.unlink(missing_ok=True)
            changed = True
        if changed:
            temporary = totals.with_name(f"totals.tsv.tmp.{os.getpid()}")
            temporary.write_text("".join(f"{v['seconds']}\t{v['last']}\t{exe}\n" for exe, v in finished.items()),
                                 encoding="utf-8", errors="surrogateescape")
            temporary.replace(totals)
    result = {exe: dict(entry) for exe, entry in finished.items()}
    for exe, start, last in live:
        add(result, exe, last - start, last)["running"] = True
    return result


def forget_playtime():
    shutil.rmtree(data_dir(), ignore_errors=True)


def duration_text(seconds):
    if seconds < 60: return _t("moins d’une minute")
    minutes = seconds // 60
    if minutes < 60: return _t("{m} min").format(m=minutes)
    return _t("{h} h {m:02d} min").format(h=minutes // 60, m=minutes % 60)


def date_text(stamp):
    local = time.localtime(stamp)
    if _LANG == "en": return time.strftime("%m/%d/%Y", local)
    if _LANG == "de": return time.strftime("%d.%m.%Y", local)
    return time.strftime("%d/%m/%Y", local)


def summary(game, times):
    if not game.get("wide", True): return _t("Programme 32 bits : pas encore pris en charge par Winlux.")
    entry = times.get(game["exe"])
    # Partie de moins de 30 s (avant le premier relevé) : lancée quand même.
    if not entry or entry["seconds"] <= 0 and not entry["running"] and not entry["last"]: return _t("Jamais lancé")
    if entry["running"]: return _t("En cours") + " · " + duration_text(entry["seconds"])
    return duration_text(entry["seconds"]) + " · " + _t("Joué le {date}").format(date=date_text(entry["last"]))


# ── Lancement et raccourcis ─────────────────────────────────────────────────

def launch(game):
    exe = Path(game["exe"])
    if not exe.is_file(): raise FileNotFoundError(str(exe))
    opener = shutil.which("winlux-open") or "/usr/bin/winlux-open"
    if not Path(opener).is_file():
        opener = shutil.which("winlux") or "/usr/bin/winlux"
    # Lancé depuis la bibliothèque : c'est un jeu, même si son .exe n'importe
    # aucune DLL graphique (Unity, Direct3D chargé à la demande). Le runtime
    # compte alors son temps de jeu ; sinon la carte restait « Jamais lancé ».
    env = dict(os.environ, WINLUX_GAME="1")
    return subprocess.Popen([opener, str(exe)], cwd=str(exe.parent), start_new_session=True, env=env,
                            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def _escaped(value):
    return str(value).replace("\\", "\\\\").replace("\n", " ").replace("\r", " ")


def _argument(value):
    value = str(value).replace("%", "%%")
    for char in ["\\", '"', "`", "$"]:
        value = value.replace(char, "\\" + char)
    return _escaped('"' + value + '"')


def launcher_text(game, icon="applications-games"):
    exe = Path(game["exe"])
    return ("[Desktop Entry]\nType=Application\n"
            f"Name={_escaped(game['name'])}\n"
            f"Exec={_argument('/usr/bin/winlux-open')} {_argument(exe)}\n"
            f"Path={_escaped(exe.parent)}\n"
            f"Icon={_escaped(icon)}\nTerminal=false\nCategories=Game;\n"
            f"X-Winlux-Executable={_escaped(exe)}\nX-Winlux-Game={game['id']}\n")


def icon_for(game):
    apps = applications_module()
    if apps is None: return "applications-games"
    try: return apps.icon_for(Path(game["exe"]), "jeu-" + game["id"])
    except Exception: return "applications-games"


def start_menu_entry(game):
    """Lanceur du menu Démarrer (catégorie Jeux) : celui de Winlux s’il existe déjà."""
    folder = applications_dir()
    try:
        for path in sorted(folder.glob("winlux-pe-*.desktop")):
            if f"X-Winlux-Executable={game['exe']}\n" in path.read_text(errors="replace"):
                return path.name
    except OSError:
        pass
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / f"winlux-game-{game['id']}.desktop"
    path.write_text(launcher_text(game, icon_for(game)), encoding="utf-8")
    return path.name


def pin(game):
    name = start_menu_entry(game)
    state_path = Path(os.environ.get("XDG_CONFIG_HOME") or str(home() / ".config")) / "winlux/start.json"
    try: state = json.loads(state_path.read_text())
    except (OSError, ValueError): state = {}
    if not isinstance(state, dict): state = {}
    pins = state.get("pins")
    if not isinstance(pins, list):
        pins = ["winlux-explorer.desktop", "winlux-browser-firefox.desktop", "org.xfce.mousepad.desktop",
                "winlux-settings.desktop", "winlux-terminal.desktop", "winlux-updates.desktop"]
    if name not in pins: pins.append(name)
    state["pins"] = pins
    state_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = state_path.with_suffix(".tmp")
    temporary.write_text(json.dumps(state, ensure_ascii=False))
    temporary.replace(state_path)
    return name


def desktop_shortcut(game):
    """Raccourci sur le bureau, marqué fiable pour xfdesktop (pas d’avertissement)."""
    folder = desktop_dir()
    folder.mkdir(parents=True, exist_ok=True)
    safe = "".join(c for c in game["name"] if c not in '/\\:*?"<>|\n\r\t').strip() or "Jeu"
    path = folder / f"{safe[:80]}.desktop"
    text = launcher_text(game, icon_for(game))
    path.write_text(text, encoding="utf-8")
    path.chmod(0o755)
    if shutil.which("gio"):
        for attribute, value in (("metadata::trusted", "true"),
                                 ("metadata::xfce-exe-checksum", hashlib.sha256(text.encode()).hexdigest())):
            subprocess.run(["gio", "set", "-t", "string", str(path), attribute, value],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    return path


def add_game(path):
    path = Path(path).resolve()
    info = pe_info(path)
    if not info: raise ValueError(_t("Ce fichier n’est pas un programme Windows."))

    def change(config):
        if str(path) not in config["ADDED"]: config["ADDED"].append(str(path))
        if str(path) in config["HIDDEN"]: config["HIDDEN"].remove(str(path))
    update_config(change)
    return path


def remove_game(exe):
    def change(config):
        if exe in config["ADDED"]: config["ADDED"].remove(exe)
        elif exe not in config["HIDDEN"]: config["HIDDEN"].append(exe)
    update_config(change)


def source_text(source):
    return _t({"gog": "GOG", "itch": "itch.io", "installed": "Installé", "folder": "Dossier de jeux",
               "manual": "Ajouté à la main"}.get(source, "Installé"))


# ── Fenêtre ─────────────────────────────────────────────────────────────────

CSS = b"""
#jeux-header { padding: 18px 24px 8px 24px; }
.jeux-title { font-size: 22px; font-weight: 600; }
.jeux-count { opacity: 0.7; }
flowboxchild { padding: 0; }
.jeux-card { border-radius: 8px; padding: 12px; background-color: alpha(@theme_fg_color, 0.04);
             border: 1px solid alpha(@theme_fg_color, 0.09); }
.jeux-card:hover { background-color: alpha(@theme_fg_color, 0.08); }
.jeux-name { font-weight: 600; }
.jeux-detail { font-size: 12px; opacity: 0.72; }
.jeux-running { color: #0f7b0f; font-weight: 600; }
.jeux-empty { font-size: 18px; font-weight: 600; }
"""


def run_window():
    import threading
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("GdkPixbuf", "2.0")
    from gi.repository import GdkPixbuf, GLib, Gtk, Gdk, Pango

    class Window(Gtk.Window):
        def __init__(self):
            super().__init__(title=_t("Jeux"))
            self.set_wmclass("winlux-games", "winlux-games")
            self.set_icon_name("applications-games")
            self.set_default_size(1000, 680)
            provider = Gtk.CssProvider()
            provider.load_from_data(CSS)
            Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider,
                                                     Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
            self.config = load_config()
            self.games = load_cache()
            self.times = {}
            self.sort = "recent"
            outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
            self.add(outer)
            header = Gtk.Box(spacing=10)
            header.set_name("jeux-header")
            outer.pack_start(header, False, False, 0)
            title = Gtk.Label(label=_t("Bibliothèque"), xalign=0)
            title.get_style_context().add_class("jeux-title")
            header.pack_start(title, False, False, 0)
            self.count = Gtk.Label(xalign=0)
            self.count.get_style_context().add_class("jeux-count")
            header.pack_start(self.count, False, False, 6)
            self.search = Gtk.SearchEntry(placeholder_text=_t("Rechercher un jeu"))
            self.search.set_width_chars(26)
            self.search.connect("search-changed", lambda *_: self.render())
            header.pack_start(self.search, True, False, 0)
            self.order = Gtk.ComboBoxText()
            for key, label in (("recent", _t("Récents")), ("name", _t("Nom")), ("time", _t("Temps de jeu"))):
                self.order.append(key, label)
            self.order.set_active_id(self.sort)
            self.order.connect("changed", lambda w: self.set_sort(w.get_active_id()))
            header.pack_end(self.order, False, False, 0)
            settings = Gtk.Button.new_from_icon_name("emblem-system-symbolic", Gtk.IconSize.BUTTON)
            settings.set_tooltip_text(_t("Réglages"))
            settings.connect("clicked", lambda *_: self.open_settings())
            header.pack_end(settings, False, False, 0)
            self.rescan_button = Gtk.Button(label=_t("Rechercher les jeux"))
            self.rescan_button.connect("clicked", lambda *_: self.rescan())
            header.pack_end(self.rescan_button, False, False, 0)
            add = Gtk.Button(label=_t("Ajouter un jeu"))
            add.get_style_context().add_class("suggested-action")
            add.connect("clicked", lambda *_: self.add_dialog())
            header.pack_end(add, False, False, 0)
            self.status = Gtk.Label(xalign=0, margin_start=24, margin_end=24)
            self.status.get_style_context().add_class("jeux-detail")
            outer.pack_start(self.status, False, False, 0)
            scroll = Gtk.ScrolledWindow()
            scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
            outer.pack_start(scroll, True, True, 0)
            self.stack = Gtk.Stack()
            scroll.add(self.stack)
            self.flow = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, homogeneous=True,
                                    column_spacing=12, row_spacing=12, margin=24, valign=Gtk.Align.START)
            self.flow.set_min_children_per_line(2)
            self.flow.set_max_children_per_line(8)
            self.flow.set_filter_func(self.visible)
            self.stack.add_named(self.flow, "games")
            empty = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin=60, valign=Gtk.Align.START)
            image = Gtk.Image.new_from_icon_name("applications-games", Gtk.IconSize.DIALOG)
            image.set_pixel_size(72)
            empty.pack_start(image, False, False, 0)
            label = Gtk.Label(label=_t("Aucun jeu pour l’instant"))
            label.get_style_context().add_class("jeux-empty")
            empty.pack_start(label, False, False, 0)
            hint = Gtk.Label(label=_t("Installez un jeu Windows (.exe), puis revenez ici. Un jeu déjà présent ailleurs s’ajoute avec « Ajouter un jeu »."),
                             wrap=True, max_width_chars=60, justify=Gtk.Justification.CENTER)
            empty.pack_start(hint, False, False, 0)
            self.stack.add_named(empty, "empty")
            self.render()
            GLib.timeout_add_seconds(30, self.tick)
            if self.config["AUTOSCAN"] == "1" or not self.games: self.rescan()

        def set_sort(self, key):
            self.sort = key or "recent"
            self.render()

        def tick(self):
            self.render(refresh_times=True)
            return True

        def ordered(self):
            games = list(self.games)
            if self.sort == "name": return sorted(games, key=lambda g: fold(g["name"]))
            key = "last" if self.sort == "recent" else "seconds"
            return sorted(games, key=lambda g: (-(self.times.get(g["exe"], {}).get(key, 0)), fold(g["name"])))

        def visible(self, child):
            query = fold(self.search.get_text().strip())
            return not query or query in fold(child.game["name"])

        def render(self, refresh_times=True):
            if refresh_times:
                try: self.times = playtime()
                except OSError: self.times = {}
            for child in self.flow.get_children(): child.destroy()
            for game in self.ordered():
                child = Gtk.FlowBoxChild()
                child.game = game
                child.add(self.card(game))
                self.flow.add(child)
            count = len(self.games)
            self.count.set_text(_t("1 jeu") if count == 1 else _t("{n} jeux").format(n=count))
            self.stack.set_visible_child_name("games" if self.games else "empty")
            self.flow.show_all()
            self.flow.invalidate_filter()

        def card(self, game):
            box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
            box.get_style_context().add_class("jeux-card")
            box.set_size_request(200, -1)
            icon = icon_for(game)
            image = None
            if icon.startswith("/"):
                try: image = Gtk.Image.new_from_pixbuf(GdkPixbuf.Pixbuf.new_from_file_at_scale(icon, 64, 64, True))
                except GLib.Error: image = None
            if image is None:
                image = Gtk.Image.new_from_icon_name("applications-games", Gtk.IconSize.DIALOG)
                image.set_pixel_size(64)
            image.set_halign(Gtk.Align.START)
            box.pack_start(image, False, False, 4)
            name = Gtk.Label(label=game["name"], xalign=0, ellipsize=Pango.EllipsizeMode.END, max_width_chars=22)
            name.set_tooltip_text(game["exe"])
            name.get_style_context().add_class("jeux-name")
            box.pack_start(name, False, False, 0)
            entry = self.times.get(game["exe"], {})
            detail = Gtk.Label(label=summary(game, self.times), xalign=0, wrap=True, max_width_chars=26)
            detail.get_style_context().add_class("jeux-running" if entry.get("running") else "jeux-detail")
            box.pack_start(detail, False, False, 0)
            source = Gtk.Label(label=source_text(game["source"]), xalign=0)
            source.get_style_context().add_class("jeux-detail")
            box.pack_start(source, False, False, 0)
            line = Gtk.Box(spacing=6)
            play = Gtk.Button(label=_t("Jouer"))
            play.get_style_context().add_class("suggested-action")
            play.set_sensitive(game.get("wide", True) and not entry.get("running"))
            play.connect("clicked", lambda *_: self.play(game))
            line.pack_start(play, True, True, 0)
            more = Gtk.MenuButton()
            more.add(Gtk.Image.new_from_icon_name("view-more-symbolic", Gtk.IconSize.BUTTON))
            more.set_popup(self.menu(game))
            line.pack_start(more, False, False, 0)
            box.pack_start(line, False, False, 0)
            events = Gtk.EventBox()
            events.add(box)
            events.connect("button-press-event", lambda _w, event: self.clicked(event, game))
            return events

        def clicked(self, event, game):
            if event.button == 3:
                self.menu(game).popup_at_pointer(event)
                return True
            if event.type == Gdk.EventType._2BUTTON_PRESS and game.get("wide", True):
                self.play(game)
                return True
            return False

        def menu(self, game):
            menu = Gtk.Menu()
            for label, action in ((_t("Jouer"), self.play), (_t("Épingler au menu Démarrer"), self.pin),
                                  (_t("Créer un raccourci sur le bureau"), self.shortcut),
                                  (_t("Ouvrir l’emplacement"), self.locate),
                                  (_t("Retirer de la bibliothèque"), self.remove)):
                item = Gtk.MenuItem(label=label)
                item.connect("activate", lambda *_, a=action: a(game))
                if action is self.play: item.set_sensitive(game.get("wide", True))
                menu.append(item)
            menu.show_all()
            return menu

        def say(self, text):
            self.status.set_text(text)

        def play(self, game):
            try:
                launch(game)
                self.say(_t("En cours") + " : " + game["name"])
                GLib.timeout_add_seconds(3, lambda: self.render() and False)
            except OSError:
                self.say(_t("Le jeu est introuvable : il a peut-être été désinstallé ou déplacé."))

        def pin(self, game):
            try:
                pin(game)
                self.say(_t("Épinglé au menu Démarrer."))
            except OSError:
                self.say(_t("Impossible de créer le raccourci."))

        def shortcut(self, game):
            try:
                desktop_shortcut(game)
                self.say(_t("Raccourci créé sur le bureau."))
            except OSError:
                self.say(_t("Impossible de créer le raccourci."))

        def locate(self, game):
            folder = str(Path(game["exe"]).parent)
            for command in (["/usr/bin/winlux-shell-app", "explorer", folder], ["xdg-open", folder]):
                if Path(command[0]).is_file() or shutil.which(command[0]):
                    subprocess.Popen(command, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    return

        def remove(self, game):
            remove_game(game["exe"])
            self.games = [g for g in self.games if g["exe"] != game["exe"]]
            save_cache(self.games)
            self.render()

        def open_settings(self):
            for command in (["/usr/bin/winlux-shell-app", "settings", str(GAMES_PAGE)],):
                if Path(command[0]).is_file():
                    subprocess.Popen(command, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        def add_dialog(self):
            dialog = Gtk.FileChooserDialog(title=_t("Choisir le programme du jeu"), transient_for=self,
                                           action=Gtk.FileChooserAction.OPEN)
            dialog.add_buttons(_t("Annuler"), Gtk.ResponseType.CANCEL, _t("Ajouter"), Gtk.ResponseType.ACCEPT)
            pattern = Gtk.FileFilter()
            pattern.set_name(_t("Programmes Windows"))
            pattern.add_pattern("*.exe")
            pattern.add_pattern("*.EXE")
            dialog.add_filter(pattern)
            if dialog.run() == Gtk.ResponseType.ACCEPT:
                try:
                    add_game(dialog.get_filename())
                    self.config = load_config()
                    self.rescan()
                except (ValueError, OSError) as error:
                    self.say(str(error))
            dialog.destroy()

        def rescan(self):
            self.rescan_button.set_sensitive(False)
            self.say(_t("Recherche des jeux installés…"))
            config = load_config()

            def work():
                try: games = scan(config)
                except Exception: games = None
                GLib.idle_add(self.scanned, games)
            threading.Thread(target=work, daemon=True).start()

        def scanned(self, games):
            self.rescan_button.set_sensitive(True)
            self.say("")
            if games is not None:
                self.games = games
                try: save_cache(games)
                except OSError: pass
            self.render()
            return False

    window = Window()
    window.connect("destroy", Gtk.main_quit)
    window.show_all()
    return window, Gtk


# Page « Jeux » des Paramètres natifs (scripts/winlux-settings.py).
GAMES_PAGE = 29


def main(argv):
    if len(argv) > 1:
        command = argv[1]
        if command == "--scan":
            games = scan()
            save_cache(games)
            print(json.dumps(games, ensure_ascii=False, indent=1))
        elif command == "--list":
            print(json.dumps(load_cache(), ensure_ascii=False, indent=1))
        elif command == "--playtime":
            print(json.dumps(playtime(), ensure_ascii=False, indent=1))
        elif command == "--add" and len(argv) > 2:
            try: print(add_game(argv[2]))
            except ValueError as error:
                print(error, file=sys.stderr)
                return 1
        elif command == "--forget-playtime":
            forget_playtime()
        else:
            print("usage : winlux-games [--scan|--list|--playtime|--add JEU.exe|--forget-playtime]", file=sys.stderr)
            return 2
        return 0
    _, Gtk = run_window()
    Gtk.main()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
