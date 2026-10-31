#!/usr/bin/python3
"""Seed native desktop defaults once; retain personal configuration afterwards."""
from pathlib import Path
import os
import shutil
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


if os.geteuid() == 0:
    try:
        pwd.getpwnam("messagebus")
    except KeyError:
        used = {entry.pw_uid for entry in pwd.getpwall()} | {entry.gr_gid for entry in grp.getgrall()}
        bus_uid = 990
        while bus_uid in used: bus_uid += 1
        with Path("/etc/group").open("a") as stream: stream.write(f"messagebus:x:{bus_uid}:\n")
        with Path("/etc/passwd").open("a") as stream: stream.write(f"messagebus:!:{bus_uid}:{bus_uid}:D-Bus:/nonexistent:/bin/false\n")
    bus_group=grp.getgrnam("messagebus")
    helper=Path("/usr/lib/dbus-1.0/dbus-daemon-launch-helper")
    if helper.is_file():
        os.chown(helper,0,bus_group.gr_gid)
        helper.chmod(0o4754)
    try: pwd.getpwnam("polkitd")
    except KeyError:
        used={entry.pw_uid for entry in pwd.getpwall()}|{entry.gr_gid for entry in grp.getgrall()}
        uid=980
        while uid in used:uid+=1
        with Path("/etc/group").open("a") as stream:stream.write(f"polkitd:x:{uid}:\n")
        with Path("/etc/passwd").open("a") as stream:stream.write(f"polkitd:!:{uid}:{uid}:PolicyKit:/nonexistent:/bin/false\n")
    policy_user=pwd.getpwnam("polkitd")
    policy_state=Path("/var/lib/polkit-1")
    policy_state.mkdir(parents=True,exist_ok=True)
    os.chown(policy_state,policy_user.pw_uid,policy_user.pw_gid)
    policy_state.chmod(0o700)
    for directory in ("/var/lib/NetworkManager","/var/lib/bluetooth","/var/lib/udisks2"):
        Path(directory).mkdir(parents=True,exist_ok=True)
    try:
        saver = pwd.getpwnam("winlux-saver")
    except KeyError:
        occupied = {entry.pw_uid for entry in pwd.getpwall()} | {entry.gr_gid for entry in grp.getgrall()}
        uid = 1002
        while uid in occupied: uid += 1
        with Path("/etc/group").open("a") as stream: stream.write(f"winlux-saver:x:{uid}:\n")
        with Path("/etc/passwd").open("a") as stream: stream.write(f"winlux-saver:!:{uid}:{uid}:Winlux screensaver:/home/winlux-saver:/bin/false\n")
        saver = pwd.getpwnam("winlux-saver")
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
        if keyboard is not None:
            keys = {binding.get("key") for binding in keyboard}
            for key, page in [("C-S-Escape", "tasks"), ("W-a", "quick"), ("W-n", "notifications")]:
                if key not in keys:
                    binding = ET.SubElement(keyboard, f"{{{namespace}}}keybind", key=key)
                    action = ET.SubElement(binding, f"{{{namespace}}}action", name="Execute")
                    ET.SubElement(action, f"{{{namespace}}}command").text = "winlux-system-tools " + page
                    changed = True
            default_rc = source / "config/openbox/rc.xml"
            if default_rc.is_file():
                defaults = ET.parse(default_rc).getroot().find(f"{{{namespace}}}keyboard")
                if defaults is not None:
                    for binding in defaults:
                        if binding.get("key") in ("W-C-Left", "W-C-Right", "W-S-Left", "W-S-Right") and binding.get("key") not in keys:
                            keyboard.append(ET.fromstring(ET.tostring(binding)))
                            changed = True
            for binding in list(keyboard):
                commands = binding.findall(f"{{{namespace}}}action/{{{namespace}}}command")
                if binding.get("key") == "Super_L" and len(commands) == 1 and commands[0].text == "winlux-start-menu":
                    keyboard.remove(binding)
                    changed = True
        if changed:
            temporary = openbox_config.with_suffix(".xml.tmp")
            tree.write(temporary, encoding="utf-8", xml_declaration=True)
            temporary.replace(openbox_config)
    except ET.ParseError:
        print(_t("Configuration Openbox personnalisée illisible : migration des raccourcis ignorée."), file=sys.stderr)
if not (home / ".xscreensaver").exists(): shutil.copyfile(source / "xscreensaver", home / ".xscreensaver")
user_dirs = home / ".config/user-dirs.dirs"
if not user_dirs.exists():
    user_dirs.write_text('XDG_DESKTOP_DIR="$HOME/Bureau"\nXDG_DOWNLOAD_DIR="$HOME/Téléchargements"\nXDG_DOCUMENTS_DIR="$HOME/Documents"\nXDG_PICTURES_DIR="$HOME/Images"\nXDG_MUSIC_DIR="$HOME/Musique"\nXDG_VIDEOS_DIR="$HOME/Vidéos"\n')
desktop = home / "Bureau"
desktop.mkdir(exist_ok=True)
# Le bureau reste vide par défaut. Les icônes (Corbeille, Explorateur,
# Documents, etc.) sont ajoutées explicitement depuis Paramètres.
# Defaults for file opening, not a rewrite of the user's selected associations.
mimeapps = home / ".config/mimeapps.list"
if not mimeapps.exists():
    mimeapps.write_text("[Default Applications]\ntext/plain=org.xfce.mousepad.desktop;\ntext/x-python=org.xfce.mousepad.desktop;\ntext/x-shellscript=org.xfce.mousepad.desktop;\ninode/directory=winlux-explorer.desktop;\napplication/x-ms-dos-executable=winlux.desktop;\napplication/x-msdownload=winlux.desktop;\napplication/vnd.microsoft.portable-executable=winlux.desktop;\nx-scheme-handler/http=winlux-browser.desktop;\nx-scheme-handler/https=winlux-browser.desktop;\n")
