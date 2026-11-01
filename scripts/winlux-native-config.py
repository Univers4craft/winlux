#!/usr/bin/python3
"""Seed native desktop defaults once; retain personal configuration afterwards."""
from pathlib import Path
import os
import shutil
import subprocess
import pwd
import grp
import sys
import xml.etree.ElementTree as ET


# ── Langue de l'interface (Paramètres > Langue) : fr, en, es ou de ──────────

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
    'Configuration Openbox personnalisée illisible : migration des raccourcis ignorée.': ('Custom Openbox configuration unreadable: shortcut migration skipped.', 'Configuración personalizada de Openbox ilegible: se omite la migración de los atajos.', 'Benutzerdefinierte Openbox-Konfiguration nicht lesbar: Übernahme der Tastenkürzel übersprungen.'),
}


def _t(text):
    """Texte dans la langue de l'interface (le français sert de clé)."""
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


def ensure_system_account(name, first_uid, comment, home):
    """Compte système « name » créé par useradd/groupadd (verrous, shadow,
    nss cohérents) ; /etc/passwd n'est complété à la main qu'en leur absence.
    Rien n'est écrit si le compte existe déjà."""
    try: return pwd.getpwnam(name)
    except KeyError: pass
    used = {entry.pw_uid for entry in pwd.getpwall()} | {entry.gr_gid for entry in grp.getgrall()}
    uid = first_uid
    while uid in used: uid += 1
    if shutil.which("groupadd") and shutil.which("useradd"):
        try: grp.getgrnam(name)
        except KeyError: subprocess.run(["groupadd", "--gid", str(uid), name], check=True)
        subprocess.run(["useradd", "--system", "--uid", str(uid), "--gid", str(uid), "--no-create-home",
                        "--home-dir", home, "--shell", "/bin/false", "--comment", comment, name], check=True)
    else:
        try: grp.getgrnam(name)
        except KeyError:
            with Path("/etc/group").open("a") as stream: stream.write(f"{name}:x:{uid}:\n")
        with Path("/etc/passwd").open("a") as stream: stream.write(f"{name}:!:{uid}:{uid}:{comment}:{home}:/bin/false\n")
    return pwd.getpwnam(name)


if os.geteuid() == 0:
    ensure_system_account("messagebus", 990, "D-Bus", "/nonexistent")
    bus_group=grp.getgrnam("messagebus")
    helper=Path("/usr/lib/dbus-1.0/dbus-daemon-launch-helper")
    if helper.is_file():
        os.chown(helper,0,bus_group.gr_gid)
        helper.chmod(0o4754)
    policy_user=ensure_system_account("polkitd", 980, "PolicyKit", "/nonexistent")
    policy_state=Path("/var/lib/polkit-1")
    policy_state.mkdir(parents=True,exist_ok=True)
    os.chown(policy_state,policy_user.pw_uid,policy_user.pw_gid)
    policy_state.chmod(0o700)
    for directory in ("/var/lib/NetworkManager","/var/lib/bluetooth","/var/lib/udisks2"):
        Path(directory).mkdir(parents=True,exist_ok=True)
    saver = ensure_system_account("winlux-saver", 1002, "Winlux screensaver", "/home/winlux-saver")
    directory = Path(saver.pw_dir)
    directory.mkdir(parents=True, exist_ok=True)
    directory.chmod(0o700)
    os.chown(directory, saver.pw_uid, saver.pw_gid)

home = Path.home()
source = Path("/usr/share/winlux/native")
reset_panel = "--reset-panel" in sys.argv
for path in (source / "config").rglob("*"):
    if path.is_file():
        target = home / ".config" / path.relative_to(source / "config")
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists() or (reset_panel and path.name == "xfce4-panel.xml"):
            shutil.copyfile(path, target)
# Raccourcis clavier : xfsettingsd applique ceux d'XFCE et les saisit avant
# openbox. Windows+E ouvrait Thunar sur « / », Windows+R/P et Ctrl+Alt+L
# visaient des programmes absents de l'image. Une valeur restée celle d'XFCE
# devient l'équivalent Winlux ; un choix personnel est conservé.
XFCE_SHORTCUTS = {
    "<Super>e": ("thunar", "winlux-shell-app explorer"),
    "<Primary><Alt>f": ("thunar", "winlux-shell-app explorer"),
    "<Super>r": ("xfce4-appfinder -c", "winlux-start-menu"),
    "<Alt>F1": ("xfce4-popup-applicationsmenu", "winlux-start-menu"),
    "<Alt>F2": ("xfce4-appfinder --collapsed", "winlux-start-menu"),
    "<Alt>F3": ("xfce4-appfinder", "winlux-start-menu"),
    "<Super>p": ("xfce4-display-settings --minimal", "winlux-shell-app settings 8"),
    "XF86Display": ("xfce4-display-settings --minimal", "winlux-shell-app settings 8"),
    "<Primary><Alt>l": ("xflock4", "winlux-lock now"),
    "<Primary><Alt>Delete": ("xfce4-session-logout", "winlux-system-tools tasks"),
    "<Primary><Shift>Escape": ("xfce4-taskmanager", "winlux-system-tools tasks"),
    "Print": ("xfce4-screenshooter", None),
    "<Alt>Print": ("xfce4-screenshooter -w", None),
    "<Shift>Print": ("xfce4-screenshooter -r", None),
    "<Alt><Super>s": ("orca", None),
}


def winlux_shortcuts(path):
    """Remplace dans le canal xfce4-keyboard-shortcuts les commandes XFCE par défaut."""
    try:
        tree = ET.parse(path) if path.is_file() else ET.ElementTree(ET.Element("channel", {"name": "xfce4-keyboard-shortcuts", "version": "1.0"}))
    except (ET.ParseError, OSError):
        return
    root = tree.getroot()

    def child(parent, name):
        found = next((item for item in parent if item.get("name") == name), None)
        if found is None:
            found = ET.SubElement(parent, "property", {"name": name, "type": "empty"})
        return found

    custom = child(child(root, "commands"), "custom")
    existing = {item.get("name"): item for item in custom}
    if len(existing) <= 1:
        # Canal pas encore créé par xfsettingsd : sans liste personnelle, il
        # recopierait les défauts d'XFCE ; garder ceux qui restent utiles.
        for key, command in (("<Primary><Alt>t", "exo-open --launch TerminalEmulator"), ("<Primary>Escape", "xfdesktop --menu"),
                             ("<Primary><Alt>Escape", "xkill"), ("XF86WWW", "exo-open --launch WebBrowser"),
                             ("HomePage", "exo-open --launch WebBrowser"), ("XF86Mail", "exo-open --launch MailReader")):
            if key not in existing:
                existing[key] = ET.SubElement(custom, "property", {"name": key, "type": "string", "value": command})
        for key, (_, command) in XFCE_SHORTCUTS.items():
            if command and key not in existing:
                existing[key] = ET.SubElement(custom, "property", {"name": key, "type": "string", "value": command})
    changed = not path.is_file()
    for key, (stock, command) in XFCE_SHORTCUTS.items():
        item = existing.get(key)
        if item is None or item.get("value") != stock:
            continue
        if command is None:
            custom.remove(item)
        else:
            item.set("value", command)
            for extra in list(item):
                item.remove(extra)
        changed = True
    override = existing.get("override")
    if override is None:
        ET.SubElement(custom, "property", {"name": "override", "type": "bool", "value": "true"})
        changed = True
    if changed:
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_name(path.name + ".tmp")
        tree.write(temporary, encoding="UTF-8", xml_declaration=True)
        temporary.replace(path)


winlux_shortcuts(home / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-keyboard-shortcuts.xml")
# Corbeille vide d'un nouveau profil : sans ces dossiers (créés d'habitude au
# premier envoi), l'Explorateur affichait « Ce dossier est introuvable ».
for directory in ("files", "info"):
    trash = home / ".local/share/Trash" / directory
    trash.mkdir(parents=True, exist_ok=True)
(home / ".local/share/Trash").chmod(0o700)
# Zone de notification des programmes Windows (Shell_NotifyIcon) : ajoutée une
# fois aux panneaux déjà configurés, juste avant les icônes réseau/son/batterie.
panel_config = home / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml"
if panel_config.is_file():
    try:
        tree = ET.parse(panel_config)
        root = tree.getroot()
        plugins = next((item for item in root if item.get("name") == "plugins"), None)
        panel = next((item for top in root if top.get("name") == "panels" for item in top if item.get("name") == "panel-1"), None)
        ids = next((item for item in panel if item.get("name") == "plugin-ids"), None) if panel is not None else None
        if plugins is not None and ids is not None and not any(item.get("value") == "systray" for item in plugins):
            used = [int(item.get("name")[7:]) for item in plugins if (item.get("name") or "").startswith("plugin-") and item.get("name")[7:].isdigit()]
            number = max(used + [0]) + 1
            tray = ET.SubElement(plugins, "property", {"name": f"plugin-{number}", "type": "string", "value": "systray"})
            ET.SubElement(tray, "property", {"name": "square-icons", "type": "bool", "value": "true"})
            ET.SubElement(tray, "property", {"name": "icon-size", "type": "uint", "value": "16"})
            genmon = {item.get("name")[7:] for item in plugins if item.get("value") == "genmon"}
            values = list(ids)
            position = next((index for index, item in enumerate(values) if item.get("value") in genmon), len(values) - 1 if values else 0)
            ids.insert(position, ET.Element("value", {"type": "int", "value": str(number)}))
            tree.write(panel_config, encoding="UTF-8", xml_declaration=True)
    except (ET.ParseError, OSError, ValueError, TypeError):
        pass
# Horloge : le greffon « clock » d'XFCE prenait la couleur de texte du thème
# (gris foncé en Adwaita clair, illisible sur la barre noire) et ouvrait un
# calendrier GTK brut. Il devient un genmon winlux-clock (texte clair fixé,
# clic = calendrier et notifications façon Windows 11), même numéro de greffon
# pour que winlux-control align-left/center garde l'ordre de la barre.
if panel_config.is_file():
    try:
        tree = ET.parse(panel_config)
        plugins = next((item for item in tree.getroot() if item.get("name") == "plugins"), None)
        changed = False
        for plugin in list(plugins) if plugins is not None else []:
            name = plugin.get("name") or ""
            if plugin.get("value") != "clock" or not name[7:].isdigit():
                continue
            plugin.set("value", "genmon")
            for child in list(plugin):
                plugin.remove(child)
            for key, kind, value in [("command", "string", "/usr/bin/winlux-clock"), ("use-label", "bool", "false"),
                                     ("text", "string", ""), ("update-period", "int", "2000"),
                                     ("font", "string", "DejaVu Sans 10")]:
                ET.SubElement(plugin, "property", {"name": key, "type": kind, "value": value})
            rc = home / ".config/xfce4/panel" / f"genmon-{name[7:]}.rc"
            rc.parent.mkdir(parents=True, exist_ok=True)
            rc.write_text("Command=/usr/bin/winlux-clock\nUseLabel=0\nText=\nUpdatePeriod=2000\nFont=DejaVu Sans 10\n", encoding="utf-8")
            changed = True
        if changed:
            tree.write(panel_config, encoding="UTF-8", xml_declaration=True)
    except (ET.ParseError, OSError, ValueError, TypeError):
        pass
# Couleurs de la barre (bloc « Winlux panel » de gtk.css) : remis à jour à
# chaque session, le reste du fichier (personnalisation) n'est pas touché.
panel_css_source = source / "config/gtk-3.0/gtk.css"
panel_css = home / ".config/gtk-3.0/gtk.css"
if panel_css_source.is_file() and panel_css.is_file():
    try:
        start, end = "/* Winlux panel */", "/* End Winlux panel */"
        shipped = panel_css_source.read_text(encoding="utf-8")
        if start in shipped and end in shipped:
            block = shipped[shipped.index(start):shipped.index(end) + len(end)]
            current = panel_css.read_text(encoding="utf-8")
            if start in current and end in current[current.index(start):]:
                updated = current[:current.index(start)] + block + current[current.index(end, current.index(start)) + len(end):]
            else:
                updated = current.rstrip("\n") + "\n" + block + "\n"
            if updated != current:
                temporary = panel_css.with_suffix(".css.tmp")
                temporary.write_text(updated, encoding="utf-8")
                temporary.replace(panel_css)
    except (OSError, UnicodeDecodeError, ValueError):
        pass
# xfce4-genmon-plugin 4.2+ (Ubuntu 26.04) lit sa configuration dans xfconf
# (/plugins/plugin-N/command...) et ignore les anciens genmon-N.rc : sans ces
# propriétés, les icônes réseau, son, batterie et notifications de la barre
# des tâches restent vides. Reporter chaque genmon-N.rc une fois.
if panel_config.is_file():
    try:
        tree = ET.parse(panel_config)
        plugins = next((item for item in tree.getroot() if item.get("name") == "plugins"), None)
        changed = False
        for plugin in list(plugins) if plugins is not None else []:
            name = plugin.get("name") or ""
            if plugin.get("value") != "genmon" or not name[7:].isdigit():
                continue
            if any(child.get("name") == "command" for child in plugin):
                continue
            rc = home / ".config/xfce4/panel" / f"genmon-{name[7:]}.rc"
            if not rc.is_file():
                continue
            values = dict(line.split("=", 1) for line in rc.read_text(encoding="utf-8").splitlines() if "=" in line)
            if not values.get("Command"):
                continue
            for key, kind, value in [("command", "string", values["Command"]),
                                     ("use-label", "bool", "true" if values.get("UseLabel") == "1" else "false"),
                                     ("text", "string", values.get("Text", "")),
                                     ("update-period", "int", str(int(values.get("UpdatePeriod", "3000") or 3000))),
                                     ("font", "string", values.get("Font", "DejaVu Sans 10"))]:
                ET.SubElement(plugin, "property", {"name": key, "type": kind, "value": value})
            changed = True
        if changed:
            tree.write(panel_config, encoding="UTF-8", xml_declaration=True)
    except (ET.ParseError, OSError, ValueError, TypeError):
        pass
# Barre des tâches : un bouton par fenêtre. Regroupées, deux fenêtres de
# l'Explorateur ne se rouvraient plus d'un clic (menu de choix à la place).
# Migration unique du réglage livré auparavant.
grouping_marker = home / ".config/winlux/taskbar-ungrouped"
if panel_config.is_file() and not grouping_marker.exists():
    try:
        tree = ET.parse(panel_config)
        plugins = next((item for item in tree.getroot() if item.get("name") == "plugins"), None)
        for plugin in list(plugins) if plugins is not None else []:
            if plugin.get("value") != "tasklist": continue
            for child in plugin:
                if child.get("name") == "grouping" and child.get("value") == "1":
                    child.set("value", "0")
        tree.write(panel_config, encoding="UTF-8", xml_declaration=True)
        grouping_marker.parent.mkdir(parents=True, exist_ok=True)
        grouping_marker.touch()
    except (ET.ParseError, OSError):
        pass
# Migrer uniquement le raccourci livré par Winlux, sans toucher aux raccourcis
# personnalisés. Super_L seul absorbe W-Left/W-Right dans Openbox.
openbox_config = home / ".config/openbox/rc.xml"
if openbox_config.is_file():
    namespace = "http://openbox.org/3.4/rc"
    ET.register_namespace("", namespace)
    try:
        tree = ET.parse(openbox_config)
        keyboard = tree.getroot().find(f"{{{namespace}}}keyboard")
        changed = False
        default_rc = source / "config/openbox/rc.xml"
        if keyboard is not None:
            keys = {binding.get("key") for binding in keyboard}
            for key, page in [("C-S-Escape", "tasks"), ("W-a", "quick"), ("W-n", "notifications")]:
                if key not in keys:
                    binding = ET.SubElement(keyboard, f"{{{namespace}}}keybind", key=key)
                    action = ET.SubElement(binding, f"{{{namespace}}}action", name="Execute")
                    ET.SubElement(action, f"{{{namespace}}}command").text = "winlux-system-tools " + page
                    changed = True
            if default_rc.is_file():
                defaults = ET.parse(default_rc).getroot().find(f"{{{namespace}}}keyboard")
                if defaults is not None:
                    for binding in defaults:
                        if binding.get("key") in ("W-C-Left", "W-C-Right", "W-S-Left", "W-S-Right", "W-l") and binding.get("key") not in keys:
                            keyboard.append(ET.fromstring(ET.tostring(binding)))
                            changed = True
            for binding in list(keyboard):
                commands = binding.findall(f"{{{namespace}}}action/{{{namespace}}}command")
                if binding.get("key") == "Super_L" and len(commands) == 1 and commands[0].text == "winlux-start-menu":
                    keyboard.remove(binding)
                    changed = True
        # Le clic droit sur le bureau ouvrait toujours le menu d'Openbox, même sur
        # un fichier (ni Renommer ni Supprimer) et le bureau n'avait jamais le
        # clavier : remplacer la liaison livrée par celle d'aujourd'hui.
        mouse = tree.getroot().find(f"{{{namespace}}}mouse")
        desktop = next((item for item in mouse if item.get("name") == "Desktop"), None) if mouse is not None else None
        if desktop is not None and default_rc.is_file() and len(desktop) == 1 and desktop[0].get("button") == "Right" \
                and desktop[0].findtext(f"{{{namespace}}}action/{{{namespace}}}menu") == "root-menu":
            default_mouse = ET.parse(default_rc).getroot().find(f"{{{namespace}}}mouse")
            position = list(mouse).index(desktop)
            mouse.remove(desktop)
            for context in [item for item in default_mouse if item.get("name") in ("Desktop", "Root")][::-1]:
                mouse.insert(position, ET.fromstring(ET.tostring(context)))
            changed = True
        if changed:
            temporary = openbox_config.with_suffix(".xml.tmp")
            tree.write(temporary, encoding="utf-8", xml_declaration=True)
            temporary.replace(openbox_config)
    except ET.ParseError:
        print(_t("Configuration Openbox personnalisée illisible : migration des raccourcis ignorée."), file=sys.stderr)
# Action « Open Terminal Here » d'origine (anglaise) du menu des dossiers et du
# bureau : remplacée par celle livrée, traduite.
uca = home / ".config/Thunar/uca.xml"
try:
    if uca.is_file() and uca.read_text(encoding="utf-8").count("<action>") == 1 and "<name>Open Terminal Here</name>" in uca.read_text(encoding="utf-8"):
        shutil.copyfile(source / "config/Thunar/uca.xml", uca)
except (OSError, UnicodeDecodeError):
    pass
# « Coller » dans le menu du bureau (clic droit) : ajouté une fois aux menus
# Openbox déjà copiés, avant « Nouveau », sans toucher au reste.
openbox_menu = home / ".config/openbox/menu.xml"
if openbox_menu.is_file():
    try:
        menu_text = openbox_menu.read_text(encoding="utf-8")
        anchor = '  <menu id="new-menu"'
        if "winlux-desktop-action paste" not in menu_text and anchor in menu_text:
            item = '  <item label="Coller"><action name="Execute"><command>winlux-desktop-action paste</command></action></item>\n'
            temporary = openbox_menu.with_suffix(".xml.tmp")
            temporary.write_text(menu_text.replace(anchor, item + anchor, 1), encoding="utf-8")
            temporary.replace(openbox_menu)
    except (OSError, UnicodeDecodeError):
        pass
if not (home / ".xscreensaver").exists(): shutil.copyfile(source / "xscreensaver", home / ".xscreensaver")
user_dirs = home / ".config/user-dirs.dirs"
if not user_dirs.exists():
    user_dirs.write_text('XDG_DESKTOP_DIR="$HOME/Bureau"\nXDG_DOWNLOAD_DIR="$HOME/Téléchargements"\nXDG_DOCUMENTS_DIR="$HOME/Documents"\nXDG_PICTURES_DIR="$HOME/Images"\nXDG_MUSIC_DIR="$HOME/Musique"\nXDG_VIDEOS_DIR="$HOME/Vidéos"\n')
desktop = home / "Bureau"
desktop.mkdir(exist_ok=True)
# Dossiers connus (C:\Users\<nom>\Downloads…) : ils doivent exister pour
# les navigateurs et les .exe qui y enregistrent directement.
for line in user_dirs.read_text(encoding="utf-8").splitlines():
    key, _, value = line.partition("=")
    value = value.strip().strip('"')
    if key.startswith("XDG_") and value.startswith("$HOME/") and len(value) > 6:
        try: (home / value[6:]).mkdir(parents=True, exist_ok=True)
        except OSError: pass
# Le bureau reste vide par défaut. Les icônes (Corbeille, Explorateur,
# Documents, etc.) sont ajoutées explicitement depuis Paramètres.
# Defaults for file opening, not a rewrite of the user's selected associations.
mimeapps = home / ".config/mimeapps.list"
if not mimeapps.exists():
    mimeapps.write_text("[Default Applications]\ntext/plain=org.xfce.mousepad.desktop;\ntext/x-python=org.xfce.mousepad.desktop;\ntext/x-shellscript=org.xfce.mousepad.desktop;\ninode/directory=winlux-explorer.desktop;\napplication/x-ms-dos-executable=winlux.desktop;\napplication/x-msdownload=winlux.desktop;\napplication/vnd.microsoft.portable-executable=winlux.desktop;\napplication/x-ms-shortcut=winlux.desktop;\ntext/x-ms-regedit=winlux-regedit.desktop;\nx-scheme-handler/http=winlux-browser.desktop;\nx-scheme-handler/https=winlux-browser.desktop;\n")
