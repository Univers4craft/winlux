#!/usr/bin/python3
"""Applications installées (#722) : tous les logiciels de l'ordinateur.

Comme Paramètres > Applications > Applications installées de Windows 11,
mais pour les deux mondes de Winlux :

  * applications Linux : lanceurs .desktop visibles, rattachés à leur paquet
    APT (taille, version, éditeur, date d'installation lus dans la base dpkg),
    Flatpak et Snap ;
  * programmes Windows : clés `Uninstall` du registre (msi, NSIS, Inno…,
    lues par `winlux --setup --programs`) et programmes portables déjà lancés
    (lanceurs winlux-pe-*.desktop de winlux-applications).

Désinstaller passe par le bon outil : APT et Snap par le relais root
(winlux-root exec, avec le Contrôle de compte d'utilisateur : pas de
systemd ni de polkit), Flatpak --user directement, les programmes Windows
par `winlux --setup --remove CLÉ`. Les composants dont Winlux a besoin
(winlux-desktop et tout ce dont il dépend) restent affichés, sans bouton
actif, comme les composants système de Windows.

Ligne de commande (tests, diagnostic) :
  winlux-installed-apps list [--json]
  winlux-installed-apps simulate ID     ce qu'APT retirerait avec ce paquet
  winlux-installed-apps uninstall ID
"""
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = os.environ.get("WINLUX_INSTALLED_ROOT", os.environ.get("WINLUX_ROOT", ""))
HOME = Path(os.environ.get("HOME") or Path.home())
DATA = Path(os.environ.get("XDG_DATA_HOME", HOME / ".local/share"))
DPKG = Path(ROOT + "/var/lib/dpkg")
# Paquets dont Winlux a besoin : eux et toutes leurs dépendances sont des
# composants du système (APT refuserait de toute façon de les retirer).
SYSTEM_ROOTS = {"winlux-desktop", "winlux-kernel"}
# Services de la session et doublons de l'Explorateur, comme le menu Démarrer.
HIDDEN_IDS = {"picom.desktop", "thunar.desktop", "org.xfce.thunar.desktop", "Thunar.desktop",
              "Thunar-folder-handler.desktop", "thunar-folder-handler.desktop", "xfce4-file-manager.desktop"}
TOKEN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9+.:~_-]{0,200}$")


def interface_lang():
    """WINLUX_LANG, sinon /etc/winlux/locale.conf (Paramètres > Langue), sinon LANG."""
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            for line in Path(ROOT + "/etc/winlux/locale.conf").read_text().splitlines():
                if line.startswith("LANG="): code = line[5:].strip()
        except OSError: pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


LANG = interface_lang()
# Texte français -> (anglais, espagnol, allemand).
_TR = {
    "Toutes les applications et tous les programmes de cet ordinateur, Linux comme Windows. Les composants dont Winlux a besoin ne peuvent pas être désinstallés.":
        ("Every app and program on this computer, Linux and Windows alike. Components Winlux needs cannot be uninstalled.",
         "Todas las aplicaciones y programas de este equipo, tanto de Linux como de Windows. Los componentes que Winlux necesita no se pueden desinstalar.",
         "Alle Apps und Programme auf diesem Computer, ob Linux oder Windows. Komponenten, die Winlux benötigt, können nicht deinstalliert werden."),
    "Rechercher une application": ("Search apps", "Buscar aplicaciones", "Apps durchsuchen"),
    "Trier par": ("Sort by", "Ordenar por", "Sortieren nach"),
    "Filtrer": ("Filter", "Filtrar", "Filtern"),
    "Nom (A à Z)": ("Name (A to Z)", "Nombre (A a Z)", "Name (A bis Z)"),
    "Nom (Z à A)": ("Name (Z to A)", "Nombre (Z a A)", "Name (Z bis A)"),
    "Taille (la plus grande)": ("Size (large to small)", "Tamaño (de mayor a menor)", "Größe (absteigend)"),
    "Taille (la plus petite)": ("Size (small to large)", "Tamaño (de menor a mayor)", "Größe (aufsteigend)"),
    "Date d’installation (récente)": ("Install date (newest)", "Fecha de instalación (reciente)", "Installationsdatum (neueste)"),
    "Date d’installation (ancienne)": ("Install date (oldest)", "Fecha de instalación (antigua)", "Installationsdatum (älteste)"),
    "Toutes les applications": ("All apps", "Todas las aplicaciones", "Alle Apps"),
    "Applications Linux": ("Linux apps", "Aplicaciones de Linux", "Linux-Apps"),
    "Programmes Windows": ("Windows programs", "Programas de Windows", "Windows-Programme"),
    "Désinstallables": ("Can be uninstalled", "Desinstalables", "Deinstallierbar"),
    "Composants du système": ("System components", "Componentes del sistema", "Systemkomponenten"),
    "{n} applications trouvées · {size}": ("{n} apps found · {size}", "{n} aplicaciones encontradas · {size}", "{n} Apps gefunden · {size}"),
    "1 application trouvée · {size}": ("1 app found · {size}", "1 aplicación encontrada · {size}", "1 App gefunden · {size}"),
    "Recherche des applications installées…": ("Looking for installed apps…", "Buscando aplicaciones instaladas…", "Installierte Apps werden gesucht…"),
    "Aucune application ne correspond.": ("No app matches.", "Ninguna aplicación coincide.", "Keine App entspricht der Suche."),
    "Désinstaller": ("Uninstall", "Desinstalar", "Deinstallieren"),
    "Ouvrir": ("Open", "Abrir", "Öffnen"),
    "Ouvrir l’emplacement": ("Open location", "Abrir ubicación", "Speicherort öffnen"),
    "Plus d’options": ("More options", "Más opciones", "Weitere Optionen"),
    "Paquet Linux (APT)": ("Linux package (APT)", "Paquete de Linux (APT)", "Linux-Paket (APT)"),
    "Flatpak": ("Flatpak", "Flatpak", "Flatpak"),
    "Snap": ("Snap", "Snap", "Snap"),
    "Programme Windows": ("Windows program", "Programa de Windows", "Windows-Programm"),
    "Programme Windows portable": ("Portable Windows program", "Programa de Windows portátil", "Portables Windows-Programm"),
    "Application de l’utilisateur": ("User app", "Aplicación del usuario", "Benutzer-App"),
    "Composant de Winlux": ("Winlux component", "Componente de Winlux", "Winlux-Komponente"),
    "Comprend aussi : {names}": ("Also includes: {names}", "Incluye también: {names}", "Enthält auch: {names}"),
    "Installé le {date}": ("Installed {date}", "Instalado el {date}", "Installiert am {date}"),
    "Winlux a besoin de ce composant : il ne peut pas être désinstallé.": (
        "Winlux needs this component: it cannot be uninstalled.",
        "Winlux necesita este componente: no se puede desinstalar.",
        "Winlux benötigt diese Komponente: Sie kann nicht deinstalliert werden."),
    "Programme portable : supprimez son dossier depuis l’Explorateur pour le retirer.": (
        "Portable program: delete its folder in Explorer to remove it.",
        "Programa portátil: elimine su carpeta desde el Explorador para quitarlo.",
        "Portables Programm: Löschen Sie seinen Ordner im Explorer, um es zu entfernen."),
    "Lanceur ajouté à la main : supprimez-le depuis son emplacement.": (
        "Launcher added by hand: delete it from its location.",
        "Lanzador añadido a mano: elimínelo desde su ubicación.",
        "Manuell hinzugefügter Starter: Löschen Sie ihn an seinem Speicherort."),
    "Installée pour tous les comptes : un administrateur doit la retirer.": (
        "Installed for all accounts: an administrator must remove it.",
        "Instalada para todas las cuentas: un administrador debe quitarla.",
        "Für alle Konten installiert: Ein Administrator muss sie entfernen."),
    "Désinstaller {name} ?": ("Uninstall {name}?", "¿Desinstalar {name}?", "{name} deinstallieren?"),
    "Cette application et ses données de programme seront retirées de l’ordinateur. Vos documents ne sont pas touchés.": (
        "This app and its program data will be removed from the computer. Your documents are not touched.",
        "Esta aplicación y sus datos de programa se quitarán del equipo. Sus documentos no se tocan.",
        "Diese App und ihre Programmdaten werden vom Computer entfernt. Ihre Dokumente bleiben unberührt."),
    "Seront aussi retirés : {names}": ("Also removed: {names}", "También se quitarán: {names}", "Ebenfalls entfernt: {names}"),
    "Désinstallation de {name}…": ("Uninstalling {name}…", "Desinstalando {name}…", "{name} wird deinstalliert…"),
    "{name} a été désinstallé.": ("{name} was uninstalled.", "{name} se ha desinstalado.", "{name} wurde deinstalliert."),
    "La désinstallation a échoué": ("Uninstall failed", "La desinstalación ha fallado", "Deinstallation fehlgeschlagen"),
    "Aucun moyen d’obtenir les droits d’administrateur sur ce système.": (
        "No way to get administrator rights on this system.",
        "No hay forma de obtener derechos de administrador en este sistema.",
        "Auf diesem System lassen sich keine Administratorrechte erlangen."),
    "Taille inconnue": ("Size unknown", "Tamaño desconocido", "Größe unbekannt"),
}


def _t(text, **values):
    columns = {"en": 0, "es": 1, "de": 2}
    if LANG in columns and text in _TR: text = _TR[text][columns[LANG]]
    return text.format(**values) if values else text


# ── Formats ────────────────────────────────────────────────────────────────

def human_size(size):
    """Taille sur disque, unités et virgule de la langue (12,3 Mo / 12.3 MB)."""
    if not size: return ""
    units = ("o", "Ko", "Mo", "Go", "To") if LANG == "fr" else ("B", "KB", "MB", "GB", "TB")
    value, unit = float(size), 0
    while value >= 1000 and unit < len(units) - 1:
        value /= 1024
        unit += 1
    text = f"{value:.0f}" if unit == 0 or value >= 100 else f"{value:.1f}"
    if LANG != "en": text = text.replace(".", ",")
    return f"{text} {units[unit]}"


def human_date(epoch):
    if not epoch: return ""
    moment = time.localtime(epoch)
    if LANG == "de": return time.strftime("%d.%m.%Y", moment)
    if LANG == "en": return time.strftime("%m/%d/%Y", moment)
    return time.strftime("%d/%m/%Y", moment)


def parse_size(text):
    """« 12.3 MB », « 1,2 GB », « 512 kB » (flatpak list) en octets."""
    match = re.match(r"^\s*([0-9]+(?:[.,][0-9]+)?)\s*([kKMGT]?)i?B?\s*$", text or "")
    if not match: return 0
    value = float(match.group(1).replace(",", "."))
    return int(value * 1000 ** " KMGT".index(match.group(2).upper() or " "))


# ── Paquets APT (base dpkg) ────────────────────────────────────────────────

def dpkg_status(path=None):
    """{paquet: champs} des paquets installés (Status: install ok installed)."""
    try: text = (path or DPKG / "status").read_text(errors="replace")
    except OSError: return {}
    packages = {}
    for block in text.split("\n\n"):
        fields, last = {}, None
        for line in block.splitlines():
            if line[:1] in (" ", "\t") and last:
                fields[last] += "\n" + line.strip()
            elif ":" in line:
                last, _, value = line.partition(":")
                fields[last] = value.strip()
        name = fields.get("Package")
        if name and fields.get("Status", "").endswith(" installed"):
            packages[name] = fields
    return packages


def relations(value):
    """Noms des paquets d'un champ Depends (toutes les alternatives)."""
    names = []
    for group in (value or "").split(","):
        for choice in group.split("|"):
            name = choice.strip().split(" ", 1)[0].split("(", 1)[0].split(":", 1)[0]
            if name: names.append(name)
    return names


def system_packages(packages):
    """Paquets dont Winlux a besoin : essentiels, protégés, requis, et la
    fermeture des dépendances de winlux-desktop et winlux-kernel."""
    provides = {}
    for name, fields in packages.items():
        for virtual in relations(fields.get("Provides", "")): provides.setdefault(virtual, set()).add(name)
    pending = [name for name, fields in packages.items()
               if name in SYSTEM_ROOTS or fields.get("Essential") == "yes" or fields.get("Protected") == "yes"
               or fields.get("Priority") == "required"]
    system = set()
    while pending:
        name = pending.pop()
        if name in system: continue
        system.add(name)
        fields = packages.get(name, {})
        for dependency in relations(fields.get("Depends", "")) + relations(fields.get("Pre-Depends", "")):
            for real in ({dependency} if dependency in packages else set()) | provides.get(dependency, set()):
                if real not in system: pending.append(real)
    return system


def desktop_owners(info=None):
    """{chemin d'un .desktop de /usr/share/applications: paquet} d'après les
    listes de fichiers de dpkg (une lecture, pas un `dpkg -S` par fichier)."""
    info = info or DPKG / "info"
    owners = {}
    try: lists = list(info.glob("*.list"))
    except OSError: return owners
    for path in lists:
        try: data = path.read_bytes()
        except OSError: continue
        if b"/share/applications/" not in data: continue
        package = path.stem.split(":", 1)[0]
        for line in data.decode("utf-8", "replace").splitlines():
            if "/share/applications/" in line and line.endswith(".desktop"):
                owners[line] = package
    return owners


def install_date(package):
    for name in (f"{package}.list", f"{package}:amd64.list", f"{package}:i386.list", f"{package}:all.list"):
        try: return int((DPKG / "info" / name).stat().st_mtime)
        except OSError: continue
    return 0


def upstream_version(version):
    """« 8:6.9.12.98+dfsg1-5.2build2 » -> « 6.9.12.98 » : la version de l'éditeur."""
    version = version.split(":", 1)[-1]
    version = version.rsplit("-", 1)[0] if "-" in version else version
    return re.split(r"[+~](?:dfsg|ds|repack|really)", version)[0]


def publisher_of(maintainer):
    """« Ubuntu Developers <…> » -> « Ubuntu Developers »."""
    return re.sub(r"\s*<[^>]*>\s*", "", maintainer or "").strip()


# ── Lanceurs .desktop ──────────────────────────────────────────────────────

def desktop_entry(path):
    """Champs de [Desktop Entry] ; Name/Comment dans la langue de l'interface."""
    try: text = path.read_text(errors="replace")
    except OSError: return None
    fields, inside = {}, False
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("["):
            inside = line == "[Desktop Entry]"
            continue
        if inside and "=" in line and not line.startswith("#"):
            key, _, value = line.partition("=")
            fields.setdefault(key.strip(), value.strip())
    for key in ("Name", "Comment", "GenericName"):
        for locale in (f"{key}[{LANG}]",):
            if fields.get(locale): fields[key] = fields[locale]
    return fields


def visible(fields):
    if not fields or fields.get("Type", "Application") != "Application": return False
    if fields.get("NoDisplay", "").lower() == "true" or fields.get("Hidden", "").lower() == "true": return False
    only = [name for name in fields.get("OnlyShowIn", "").split(";") if name]
    if only and "XFCE" not in only: return False
    return "XFCE" not in fields.get("NotShowIn", "").split(";")


def application_dirs():
    """Ordre de priorité XDG : l'utilisateur d'abord, comme GIO."""
    dirs = [DATA / "applications"]
    for base in (os.environ.get("XDG_DATA_DIRS") or "/usr/local/share:/usr/share").split(":"):
        if base: dirs.append(Path(ROOT + base.rstrip("/")) / "applications")
    dirs += [DATA / "flatpak/exports/share/applications", Path(ROOT + "/var/lib/flatpak/exports/share/applications"),
             Path(ROOT + "/var/lib/snapd/desktop/applications")]
    seen, unique = set(), []
    for directory in dirs:
        if str(directory) not in seen:
            seen.add(str(directory))
            unique.append(directory)
    return unique


def desktop_files():
    """[(identifiant, chemin, champs)] des lanceurs, le premier dossier gagnant."""
    found = {}
    for directory in application_dirs():
        if not directory.is_dir(): continue
        for path in sorted(directory.rglob("*.desktop")):
            ident = str(path.relative_to(directory)).replace("/", "-")
            if ident in found: continue
            found[ident] = (path, desktop_entry(path))
    return [(ident, path, fields) for ident, (path, fields) in found.items()]


def system_path(path):
    """Chemin vu par dpkg (sans le préfixe de test WINLUX_INSTALLED_ROOT)."""
    text = str(path)
    return text[len(ROOT):] if ROOT and text.startswith(ROOT) else text


# ── Programmes Windows ─────────────────────────────────────────────────────

def applications_module():
    """winlux-applications (registre, icônes PE), facultatif."""
    from importlib.machinery import SourceFileLoader
    import importlib.util
    here = Path(__file__).resolve().parent
    for candidate in (os.environ.get("WINLUX_APPLICATIONS", ""), "/usr/bin/winlux-applications",
                      str(here / "winlux-applications.py")):
        if candidate and Path(candidate).is_file():
            try:
                loader = SourceFileLoader("winlux_applications_installed", candidate)
                spec = importlib.util.spec_from_loader("winlux_applications_installed", loader)
                module = importlib.util.module_from_spec(spec)
                loader.exec_module(module)
                return module
            except Exception:
                return None
    return None


def runtime_programs():
    """Lignes app=CLÉ\tnom\tversion\téditeur\tKio\tAAAAMMJJ\ttype du runtime."""
    if not shutil.which("winlux"): return []
    try:
        out = subprocess.run(["winlux", "--setup", "--programs"], capture_output=True, text=True,
                             errors="replace", timeout=30).stdout
    except (OSError, subprocess.SubprocessError):
        return []
    programs = []
    for line in out.splitlines():
        if not line.startswith("app="): continue
        parts = line[4:].split("\t")
        if len(parts) >= 7 and parts[0] and parts[1]: programs.append(parts[:7])
    return programs


def folder_size(path, limit=200000):
    total = 0
    try:
        for count, (current, _, files) in enumerate(os.walk(path)):
            if count > limit: break
            for name in files:
                try: total += os.lstat(os.path.join(current, name)).st_size
                except OSError: pass
    except OSError: pass
    return total


def date_of(text):
    try: return int(time.mktime(time.strptime(text[:8], "%Y%m%d")))
    except (ValueError, OverflowError): return 0


def windows_apps(launchers, module):
    """Programmes inscrits dans le registre, puis programmes portables."""
    registry = {}
    if module is not None:
        try: registry = module.registry_strings(HOME)
        except Exception: registry = {}

    def host(value):
        if not value or module is None: return None
        try: return Path(module.windows_path(module.expand(value), HOME))
        except Exception: return None

    apps, claimed = [], set()
    for key, name, version, publisher, size_kb, date, kind in runtime_programs():
        values = registry.get(key.lower(), {})
        icon_exe = host(values.get("displayicon", "").strip('"').rsplit(",", 1)[0])
        location = host(values.get("installlocation", "").strip('"'))
        if location is not None and not location.is_dir(): location = None
        if location is None and icon_exe is not None: location = icon_exe.parent
        size = int(size_kb or 0) * 1024 if str(size_kb).isdigit() else 0
        if not size and location is not None and str(location) not in (str(HOME), "/"): size = folder_size(location)
        icon = None
        for launcher_exe, launcher in launchers.items():
            inside = location is not None and str(launcher_exe).startswith(str(location).rstrip("/") + "/")
            if launcher_exe == icon_exe or inside or launcher[1].get("Name", "").casefold() == name.casefold():
                claimed.add(launcher_exe)
                icon = icon or launcher[1].get("Icon")
        apps.append({"id": "windows:" + key, "name": name, "kind": "windows", "version": version,
                     "publisher": publisher, "size": size, "date": date_of(date), "icon": icon,
                     "pe_icon": str(icon_exe) if icon_exe and not icon else None,
                     "location": str(location) if location else None, "launch": None,
                     "removable": True, "system": False, "detail": kind})
    for exe, (path, fields) in launchers.items():
        if exe in claimed: continue
        try: stat = exe.stat()
        except OSError: continue
        apps.append({"id": "exe:" + str(exe), "name": fields.get("Name") or exe.stem, "kind": "portable",
                     "version": "", "publisher": "", "size": stat.st_size, "date": int(stat.st_mtime),
                     "icon": fields.get("Icon"), "pe_icon": None, "location": str(exe.parent),
                     "launch": str(path), "removable": False, "system": False, "detail": ""})
    return apps


# ── Flatpak et Snap ────────────────────────────────────────────────────────

def flatpak_apps(desktops):
    if not shutil.which("flatpak"): return []
    env = {**os.environ, "LC_ALL": "C"}
    columns = "application,name,version,origin,installation,size"
    try:
        result = subprocess.run(["flatpak", "list", "--app", f"--columns={columns}"], capture_output=True,
                                text=True, errors="replace", timeout=20, env=env)
        if result.returncode:
            columns = "application,name,version,origin,installation"
            result = subprocess.run(["flatpak", "list", "--app", f"--columns={columns}"], capture_output=True,
                                    text=True, errors="replace", timeout=20, env=env)
    except (OSError, subprocess.SubprocessError):
        return []
    apps = []
    for line in result.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) < 5 or not TOKEN.match(parts[0]): continue
        ident, name, version, origin, installation = parts[:5]
        user = installation.strip().lower() == "user"
        base = (DATA / "flatpak/app" if user else Path(ROOT + "/var/lib/flatpak/app")) / ident
        try: date = int(base.stat().st_mtime)
        except OSError: date = 0
        desktop = desktops.get(ident + ".desktop")
        apps.append({"id": f"flatpak:{'user' if user else 'system'}:{ident}",
                     "name": (desktop[1].get("Name") if desktop else None) or name or ident, "kind": "flatpak",
                     "version": version, "publisher": origin, "size": parse_size(parts[5]) if len(parts) > 5 else 0,
                     "date": date, "icon": (desktop[1].get("Icon") if desktop else None) or ident, "pe_icon": None,
                     "location": str(base) if base.is_dir() else None,
                     "launch": str(desktop[0]) if desktop else None,
                     "removable": user or os.geteuid() == 0, "system": False, "detail": ident})
    return apps


def snap_apps(desktops):
    if not shutil.which("snap"): return []
    try:
        out = subprocess.run(["snap", "list"], capture_output=True, text=True, errors="replace",
                             timeout=20, env={**os.environ, "LC_ALL": "C"}).stdout
    except (OSError, subprocess.SubprocessError):
        return []
    apps = []
    for line in out.splitlines()[1:]:
        parts = line.split()
        if len(parts) < 5 or not TOKEN.match(parts[0]): continue
        name, version, revision, publisher = parts[0], parts[1], parts[2], parts[4]
        notes = parts[5] if len(parts) > 5 else ""
        launchers = [(ident, item) for ident, item in desktops.items() if ident.startswith(name + "_")]
        if not launchers: continue  # bases, snapd, runtimes : pas des applications
        ident, (path, fields) = sorted(launchers)[0]
        snap_file = Path(ROOT + f"/var/lib/snapd/snaps/{name}_{revision}.snap")
        try: stat = snap_file.stat()
        except OSError: stat = None
        apps.append({"id": "snap:" + name, "name": fields.get("Name") or name, "kind": "snap", "version": version,
                     "publisher": publisher.rstrip("*✓").strip(), "size": stat.st_size if stat else 0,
                     "date": int(stat.st_mtime) if stat else 0, "icon": fields.get("Icon"), "pe_icon": None,
                     "location": None, "launch": str(path), "removable": "core" not in notes and "base" not in notes,
                     "system": False, "detail": name})
    return apps


# ── Liste complète ─────────────────────────────────────────────────────────

def collect():
    """Toutes les applications installées, une ligne par logiciel."""
    packages = dpkg_status()
    system = system_packages(packages)
    owners = desktop_owners()
    entries = desktop_files()
    by_id = {ident: (path, fields) for ident, path, fields in entries}
    module = applications_module()
    launchers, groups, local = {}, {}, []
    for ident, path, fields in entries:
        if not fields: continue
        exe = fields.get("X-Winlux-Executable")
        if ident.startswith("winlux-pe-") and exe:
            if visible(fields): launchers[Path(exe)] = (path, fields)
            continue
        if not visible(fields) or ident in HIDDEN_IDS: continue
        if "X-Winlux-Generated-Hash" in fields or "X-Winlux-Lnk" in fields: continue
        text = str(path)
        if "/flatpak/exports/" in text or "/snapd/desktop/" in text: continue
        package = owners.get(system_path(path))
        if package:
            groups.setdefault(package, []).append((ident, path, fields))
        else:
            # Lanceur de l'utilisateur, ou posé hors de dpkg (image Winlux).
            local.append((ident, path, fields))
    apps = []
    for package, items in groups.items():
        # Le lanceur qui porte le nom du paquet le représente (xterm.desktop
        # plutôt que uxterm.desktop), sinon le premier par ordre alphabétique.
        def named(ident):
            stem = ident.removesuffix(".desktop").lower()
            return stem == package or stem.endswith(("-" + package, "." + package))
        items.sort(key=lambda item: (not named(item[0]),
                                     (item[2].get("Name") or item[0]).casefold()))
        ident, path, fields = items[0]
        info = packages.get(package, {})
        try: size = int(info.get("Installed-Size", "0")) * 1024
        except ValueError: size = 0
        is_system = package in system or package.startswith("winlux")
        apps.append({"id": "apt:" + package, "name": fields.get("Name") or package, "kind": "apt",
                     "version": upstream_version(info.get("Version", "")), "publisher": publisher_of(info.get("Maintainer")),
                     "size": size, "date": install_date(package), "icon": fields.get("Icon"), "pe_icon": None,
                     "location": None, "launch": str(path), "removable": not is_system, "system": is_system,
                     "detail": package, "also": [item[2].get("Name") or item[0] for item in items[1:]]})
    for ident, path, fields in local:
        mine = path.is_relative_to(DATA)
        winlux = ident.startswith("winlux") or not mine
        try: stat = path.stat()
        except OSError: continue
        apps.append({"id": "desktop:" + str(path), "name": fields.get("Name") or ident, "kind": "local",
                     "version": "", "publisher": "Winlux" if winlux and not mine else "", "size": 0,
                     "date": int(stat.st_mtime), "icon": fields.get("Icon"), "pe_icon": None,
                     "location": str(path.parent), "launch": str(path), "removable": False,
                     "system": not mine, "detail": ident})
    apps += flatpak_apps(by_id)
    apps += snap_apps(by_id)
    apps += windows_apps(launchers, module)
    for app in apps:
        app.setdefault("also", [])
        app["search"] = " ".join([app["name"], app["publisher"], app["detail"] or "", *app["also"]]).casefold()
    return sorted(apps, key=lambda app: app["name"].casefold())


# ── Désinstallation ────────────────────────────────────────────────────────

def admin(command):
    """Commande root : directe, sinon relais root de Winlux (Contrôle de
    compte d'utilisateur), sinon pkexec. None si aucun moyen."""
    if os.geteuid() == 0: return command
    if shutil.which("winlux-root") and os.path.exists(os.environ.get("WINLUX_ROOT_SOCKET", "/run/winlux-root.sock")):
        return ["winlux-root", "exec", *command]
    if shutil.which("pkexec"): return ["pkexec", *command]
    return None


def uninstall_command(ident):
    """Commande qui désinstalle l'application `ident` ; None si impossible."""
    kind, _, value = ident.partition(":")
    if kind == "apt" and TOKEN.match(value):
        return admin(["apt-get", "-y", "remove", value])
    if kind == "snap" and TOKEN.match(value):
        return admin(["snap", "remove", value])
    if kind == "flatpak":
        scope, _, app = value.partition(":")
        if not TOKEN.match(app): return None
        if scope == "user": return ["flatpak", "uninstall", "--user", "-y", "--noninteractive", app]
        return ["flatpak", "uninstall", "--system", "-y", "--noninteractive", app] if os.geteuid() == 0 else None
    if kind == "windows" and value and "\n" not in value:
        return ["winlux", "--setup", "--remove", value]
    return None


def simulate(package):
    """Paquets qu'APT retirerait avec `package` (simulation, sans droits)."""
    if not TOKEN.match(package) or not shutil.which("apt-get"): return []
    try:
        out = subprocess.run(["apt-get", "-s", "remove", package], capture_output=True, text=True,
                             errors="replace", timeout=60, env={**os.environ, "LC_ALL": "C"}).stdout
    except (OSError, subprocess.SubprocessError):
        return []
    return [line.split()[1] for line in out.splitlines() if line.startswith("Remv ") and len(line.split()) > 1]


def removal_check(app):
    """(autorisé, autres paquets retirés) avant de confirmer."""
    if not app.get("removable"): return False, []
    if app["kind"] != "apt": return True, []
    package = app["detail"]
    removed = simulate(package)
    system = system_packages(dpkg_status())
    if any(name in system for name in removed): return False, []
    return True, [name for name in removed if name != package]


def main(argv):
    if len(argv) >= 2 and argv[1] == "list":
        apps = collect()
        if "--json" in argv:
            print(json.dumps(apps, ensure_ascii=False, indent=1))
        else:
            for app in apps:
                print("\t".join([app["id"], app["name"], app["kind"], app["version"], str(app["size"]),
                                 human_date(app["date"]), "système" if app["system"] else
                                 ("désinstallable" if app["removable"] else "-")]))
        return 0
    if len(argv) == 3 and argv[1] == "simulate":
        print("\n".join(simulate(argv[2].removeprefix("apt:"))))
        return 0
    if len(argv) == 3 and argv[1] == "uninstall":
        app = next((item for item in collect() if item["id"] == argv[2]), None)
        if app is None or not removal_check(app)[0]:
            print(_t("Winlux a besoin de ce composant : il ne peut pas être désinstallé."), file=sys.stderr)
            return 1
        command = uninstall_command(argv[2])
        if command is None:
            print(_t("Aucun moyen d’obtenir les droits d’administrateur sur ce système."), file=sys.stderr)
            return 1
        os.environ.setdefault("WINLUX_ROOT_APP", "Paramètres")
        return subprocess.call(command)
    print(__doc__.strip().split("Ligne de commande", 1)[1], file=sys.stderr)
    return 2


# ── Page des Paramètres ────────────────────────────────────────────────────

def InstalledAppsPanel(window, kind=None):
    """Contenu de Paramètres > Applications > Applications installées ;
    `kind` (« windows »…) présélectionne un filtre."""
    import threading
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import GdkPixbuf, Gio, GLib, Gtk, Pango

    SORTS = [("name", _t("Nom (A à Z)")), ("name-desc", _t("Nom (Z à A)")), ("size", _t("Taille (la plus grande)")),
             ("size-asc", _t("Taille (la plus petite)")), ("date", _t("Date d’installation (récente)")),
             ("date-asc", _t("Date d’installation (ancienne)"))]
    FILTERS = [("all", _t("Toutes les applications")), ("linux", _t("Applications Linux")),
               ("windows", _t("Programmes Windows")), ("removable", _t("Désinstallables")),
               ("system", _t("Composants du système"))]
    KINDS = {"apt": _t("Paquet Linux (APT)"), "flatpak": _t("Flatpak"), "snap": _t("Snap"),
             "windows": _t("Programme Windows"), "portable": _t("Programme Windows portable"),
             "local": _t("Application de l’utilisateur")}

    class Panel(Gtk.Box):
        def __init__(self):
            super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=10)
            self.set_name("winlux-installed-apps")
            self.window = window
            self.apps = []
            self.rows = {}
            self.busy = None
            self.module = None
            # Page quittée pendant la recherche : rien à remplir.
            self.alive = True
            self.connect("destroy", lambda *_: setattr(self, "alive", False))
            intro = Gtk.Label(label=_t("Toutes les applications et tous les programmes de cet ordinateur, Linux comme Windows. Les composants dont Winlux a besoin ne peuvent pas être désinstallés."),
                              xalign=0, wrap=True)
            intro.get_style_context().add_class("subtitle")
            self.pack_start(intro, False, False, 0)
            self.search = Gtk.SearchEntry(placeholder_text=_t("Rechercher une application"))
            self.search.connect("search-changed", lambda *_: self.refilter())
            self.pack_start(self.search, False, False, 0)
            tools = Gtk.FlowBox(selection_mode=Gtk.SelectionMode.NONE, max_children_per_line=2,
                                column_spacing=12, row_spacing=6, homogeneous=False)
            self.sort = self.combo(tools, _t("Trier par"), SORTS)
            self.filter = self.combo(tools, _t("Filtrer"), FILTERS)
            self.pack_start(tools, False, False, 0)
            self.count = Gtk.Label(label=_t("Recherche des applications installées…"), xalign=0)
            self.count.get_style_context().add_class("subtitle")
            self.pack_start(self.count, False, False, 0)
            self.notice = Gtk.Label(xalign=0, wrap=True)
            self.pack_start(self.notice, False, False, 0)
            self.list = Gtk.ListBox(selection_mode=Gtk.SelectionMode.NONE)
            self.list.get_style_context().add_class("card")
            self.list.set_filter_func(self.matches)
            self.list.set_sort_func(self.compare)
            placeholder = Gtk.Label(label=_t("Aucune application ne correspond."), margin=18)
            placeholder.show()
            self.list.set_placeholder(placeholder)
            if kind in dict(FILTERS): self.filter.set_active_id(kind)
            self.pack_start(self.list, False, False, 0)
            self.show_all()
            self.notice.hide()
            self.reload()

        def combo(self, parent, title, items):
            line = Gtk.Box(spacing=8)
            line.pack_start(Gtk.Label(label=title), False, False, 0)
            widget = Gtk.ComboBoxText()
            for key, label in items: widget.append(key, label)
            widget.set_active(0)
            widget.connect("changed", lambda *_: (self.list.invalidate_sort(), self.refilter()))
            line.pack_start(widget, False, False, 0)
            parent.add(line)
            return widget

        # ── Chargement ──

        def reload(self):
            def work():
                try: apps = collect()
                except Exception as error:  # la page reste ouverte, le reste fonctionne
                    print(f"winlux-installed-apps : {error}", file=sys.stderr)
                    apps = []
                GLib.idle_add(self.populate, apps)
            threading.Thread(target=work, daemon=True).start()

        def populate(self, apps):
            if not self.alive: return False
            for child in self.list.get_children(): child.destroy()
            self.apps, self.rows = apps, {}
            for app in apps:
                row = self.build_row(app)
                self.rows[app["id"]] = row
                self.list.add(row)
            self.list.show_all()
            self.refilter()
            return False

        def icon_for(self, app):
            size = 32
            icon = app.get("icon")
            try:
                if icon and icon.startswith("/") and Path(icon).is_file():
                    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(icon, size, size, True)
                    return Gtk.Image.new_from_pixbuf(pixbuf)
                if icon and Gtk.IconTheme.get_default().has_icon(icon):
                    image = Gtk.Image.new_from_icon_name(icon, Gtk.IconSize.DND)
                    image.set_pixel_size(size)
                    return image
                if app.get("pe_icon"):
                    if self.module is None: self.module = applications_module() or False
                    data = self.module.pe_icon(Path(app["pe_icon"])) if self.module else None
                    if data:
                        loader = GdkPixbuf.PixbufLoader.new_with_type("ico")
                        loader.write(data)
                        loader.close()
                        pixbuf = loader.get_pixbuf()
                        if pixbuf: return Gtk.Image.new_from_pixbuf(pixbuf.scale_simple(size, size, GdkPixbuf.InterpType.BILINEAR))
            except Exception:
                pass
            image = Gtk.Image.new_from_icon_name("application-x-executable", Gtk.IconSize.DND)
            image.set_pixel_size(size)
            return image

        def build_row(self, app):
            row = Gtk.ListBoxRow(activatable=False)
            row.app = app
            line = Gtk.Box(spacing=12, margin=8)
            line.pack_start(self.icon_for(app), False, False, 0)
            texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.CENTER)
            name = Gtk.Label(label=app["name"], xalign=0, ellipsize=Pango.EllipsizeMode.END)
            bold = Pango.AttrList()
            bold.insert(Pango.attr_weight_new(Pango.Weight.SEMIBOLD))
            name.set_attributes(bold)
            texts.pack_start(name, False, False, 0)
            kind = _t("Composant de Winlux") if app["system"] and app["kind"] == "local" else KINDS.get(app["kind"], "")
            parts = [part for part in (app["version"], app["publisher"], kind) if part]
            if app["date"]: parts.append(_t("Installé le {date}", date=human_date(app["date"])))
            detail = Gtk.Label(label=" · ".join(parts), xalign=0, ellipsize=Pango.EllipsizeMode.END)
            detail.get_style_context().add_class("subtitle")
            texts.pack_start(detail, False, False, 0)
            if app["also"]:
                also = Gtk.Label(label=_t("Comprend aussi : {names}", names=", ".join(app["also"][:6])
                                          + ("…" if len(app["also"]) > 6 else "")),
                                 xalign=0, ellipsize=Pango.EllipsizeMode.END)
                also.get_style_context().add_class("subtitle")
                texts.pack_start(also, False, False, 0)
            line.pack_start(texts, True, True, 0)
            size = Gtk.Label(label=human_size(app["size"]), xalign=1, width_chars=8)
            line.pack_start(size, False, False, 0)
            button = Gtk.Button(label=_t("Désinstaller"))
            button.set_sensitive(bool(app["removable"]))
            if not app["removable"]:
                why = (_t("Winlux a besoin de ce composant : il ne peut pas être désinstallé.") if app["system"] else
                       _t("Programme portable : supprimez son dossier depuis l’Explorateur pour le retirer.") if app["kind"] == "portable" else
                       _t("Installée pour tous les comptes : un administrateur doit la retirer.") if app["kind"] == "flatpak" else
                       _t("Lanceur ajouté à la main : supprimez-le depuis son emplacement."))
                button.set_tooltip_text(why)
            button.connect("clicked", lambda *_: self.uninstall(app))
            row.uninstall_button = button
            line.pack_start(button, False, False, 0)
            menu = Gtk.Menu()
            if app.get("launch"):
                item = Gtk.MenuItem(label=_t("Ouvrir"))
                item.connect("activate", lambda *_: self.launch(app))
                menu.append(item)
            if app.get("location"):
                item = Gtk.MenuItem(label=_t("Ouvrir l’emplacement"))
                item.connect("activate", lambda *_: self.spawn(["winlux-shell-app", "explorer", app["location"]]))
                menu.append(item)
            more = Gtk.MenuButton(popup=menu)
            more.add(Gtk.Label(label="⋯"))
            more.set_tooltip_text(_t("Plus d’options"))
            more.set_relief(Gtk.ReliefStyle.NONE)
            more.set_sensitive(bool(menu.get_children()))
            menu.show_all()
            line.pack_start(more, False, False, 0)
            row.add(line)
            return row

        # ── Recherche, tri, filtre ──

        def matches(self, row):
            app = row.app
            query = self.search.get_text().strip().casefold()
            if query and query not in app["search"]: return False
            wanted = self.filter.get_active_id()
            if wanted == "linux": return app["kind"] in ("apt", "flatpak", "snap", "local")
            if wanted == "windows": return app["kind"] in ("windows", "portable")
            if wanted == "removable": return bool(app["removable"])
            if wanted == "system": return bool(app["system"])
            return True

        def compare(self, first, second):
            a, b = first.app, second.app
            order = self.sort.get_active_id() or "name"
            names = (a["name"].casefold() > b["name"].casefold()) - (a["name"].casefold() < b["name"].casefold())
            if order.startswith("size"): key = (a["size"] > b["size"]) - (a["size"] < b["size"])
            elif order.startswith("date"): key = (a["date"] > b["date"]) - (a["date"] < b["date"])
            else: key = names
            if order in ("size", "date", "name-desc"): key = -key
            return key or names

        def refilter(self):
            self.list.invalidate_filter()
            shown = [row.app for row in self.list.get_children() if self.matches(row)]
            total = human_size(sum(app["size"] for app in shown)) or "0"
            self.count.set_text(_t("1 application trouvée · {size}", size=total) if len(shown) == 1 else
                                _t("{n} applications trouvées · {size}", n=len(shown), size=total))

        # ── Actions ──

        def spawn(self, args):
            try: subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            except OSError as error: self.message(Gtk.MessageType.ERROR, _t("La désinstallation a échoué"), str(error))

        def launch(self, app):
            try: Gio.DesktopAppInfo.new_from_filename(app["launch"]).launch([], None)
            except (GLib.Error, TypeError, AttributeError) as error: print(f"winlux-installed-apps : {error}", file=sys.stderr)

        def message(self, kind, text, detail=None, buttons=Gtk.ButtonsType.CLOSE):
            dialog = Gtk.MessageDialog(transient_for=self.window, modal=True, message_type=kind, buttons=buttons, text=text)
            if detail: dialog.format_secondary_text(detail)
            answer = dialog.run()
            dialog.destroy()
            return answer

        def uninstall(self, app):
            if self.busy is not None: return
            allowed, others = removal_check(app)
            if not allowed:
                self.message(Gtk.MessageType.INFO, app["name"], _t("Winlux a besoin de ce composant : il ne peut pas être désinstallé."))
                return
            detail = _t("Cette application et ses données de programme seront retirées de l’ordinateur. Vos documents ne sont pas touchés.")
            if others: detail += "\n\n" + _t("Seront aussi retirés : {names}", names=", ".join(others[:20]) + ("…" if len(others) > 20 else ""))
            if self.message(Gtk.MessageType.QUESTION, _t("Désinstaller {name} ?", name=app["name"]), detail,
                            Gtk.ButtonsType.OK_CANCEL) != Gtk.ResponseType.OK:
                return
            command = uninstall_command(app["id"])
            if command is None:
                self.message(Gtk.MessageType.ERROR, _t("La désinstallation a échoué"),
                             _t("Aucun moyen d’obtenir les droits d’administrateur sur ce système."))
                return
            env = {**os.environ, "WINLUX_ROOT_APP": "Paramètres", "DEBIAN_FRONTEND": "noninteractive"}
            try:
                process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                           stdin=subprocess.DEVNULL, text=True, errors="replace", env=env)
            except OSError as error:
                self.message(Gtk.MessageType.ERROR, _t("La désinstallation a échoué"), str(error))
                return
            self.busy = process
            output = []
            threading.Thread(target=lambda: output.append(process.stdout.read()), daemon=True).start()
            self.notice.set_text(_t("Désinstallation de {name}…", name=app["name"]))
            self.notice.show()
            for row in self.list.get_children(): row.uninstall_button.set_sensitive(False)

            def wait():
                if process.poll() is None: return True
                self.busy = None
                if not self.alive: return False
                if process.returncode:
                    self.notice.hide()
                    self.message(Gtk.MessageType.ERROR, _t("La désinstallation a échoué"),
                                 ("".join(output).strip() or f"code {process.returncode}")[-2000:])
                else:
                    self.notice.set_text(_t("{name} a été désinstallé.", name=app["name"]))
                self.reload()
                return False
            GLib.timeout_add(300, wait)

    return Panel()


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
