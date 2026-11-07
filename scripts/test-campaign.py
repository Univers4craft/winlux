#!/usr/bin/python3
"""Régressions de la campagne de tests v0.2.13 (VM neuve, novembre 2026).

Chaque vérification reprend un défaut trouvé en VM puis corrigé : s'il
revient, ce test échoue. Tout tourne sur la machine de développement, sans
VM ni droits root (profils isolés dans des dossiers temporaires, outils
système remplacés par des faux qui notent leurs arguments).
"""
import hashlib
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from types import SimpleNamespace
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
NATIVE = ROOT / "integration/native"
OB = "{http://openbox.org/3.4/rc}"
checks = 0


def ok(condition, label):
    global checks
    if not condition:
        print(f"ÉCHEC : {label}", file=sys.stderr)
        raise SystemExit(1)
    checks += 1
    print(f"  OK     {label}")


def native_config(home):
    env = {**os.environ, "HOME": str(home), "WINLUX_NATIVE_SOURCE": str(NATIVE)}
    subprocess.run([sys.executable, "-B", str(SCRIPTS / "winlux-native-config.py")], env=env, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def shortcuts(home):
    path = home / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-keyboard-shortcuts.xml"
    root = ET.parse(path).getroot()
    commands = next(item for item in root if item.get("name") == "commands")
    custom = next(item for item in commands if item.get("name") == "custom")
    return {item.get("name"): item.get("value") for item in custom}


def fake_tool(directory, name, body):
    tool = directory / name
    tool.write_text("#!/bin/sh\n" + body + "\n")
    tool.chmod(0o755)
    return tool


# ── Raccourcis Windows (Windows+E ouvrait Thunar en root sur « / ») ─────────
with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    native_config(home)
    keys = shortcuts(home)
    ok(keys.get("<Super>e") == "winlux-shell-app explorer", "profil neuf : Windows+E ouvre l'Explorateur Winlux")
    ok(keys.get("<Super>r") == "winlux-run", "profil neuf : Windows+R ouvre la boîte Exécuter")
    ok(keys.get("<Super>p") == "winlux-shell-app settings 8", "profil neuf : Windows+P ouvre Paramètres > Écran")
    ok(keys.get("<Primary><Alt>l") == "winlux-lock now", "profil neuf : Ctrl+Alt+L verrouille")
    ok(keys.get("<Primary><Shift>Escape") == "winlux-system-tools tasks", "profil neuf : Ctrl+Maj+Échap ouvre le Gestionnaire des tâches")
    ok(keys.get("override") == "true", "profil neuf : xfsettingsd ne recopie pas les défauts d'XFCE")
    ok("<Alt><Super>s" not in keys and "Print" not in keys, "profil neuf : aucun raccourci vers un programme absent (orca, screenshooter)")
    ok("<Primary><Alt>t" not in keys, "Ctrl+Alt+T laissé à openbox (actif dès l'apparition du bureau)")

with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    channel = home / ".config/xfce4/xfconf/xfce-perchannel-xml"
    channel.mkdir(parents=True)
    (channel / "xfce4-keyboard-shortcuts.xml").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n<channel name="xfce4-keyboard-shortcuts" version="1.0">'
        '<property name="commands" type="empty"><property name="custom" type="empty">'
        '<property name="&lt;Super&gt;e" type="string" value="thunar"/>'
        '<property name="&lt;Super&gt;r" type="string" value="xfce4-appfinder -c"><property name="startup-notify" type="bool" value="true"/></property>'
        '<property name="&lt;Super&gt;p" type="string" value="mon-outil"/>'
        '<property name="Print" type="string" value="xfce4-screenshooter"/>'
        '<property name="override" type="bool" value="true"/></property></property></channel>\n')
    native_config(home)
    keys = shortcuts(home)
    ok(keys.get("<Super>e") == "winlux-shell-app explorer", "profil existant : Windows+E migré vers l'Explorateur")
    ok(keys.get("<Super>r") == "winlux-run", "profil existant : Windows+R migré vers Exécuter (option startup-notify retirée)")
    ok(keys.get("<Super>p") == "mon-outil", "profil existant : un raccourci personnalisé est conservé")
    ok("Print" not in keys, "profil existant : Impr écran vers un programme absent retiré")

# ── Bureau : menu clic droit des fichiers et clavier (Ctrl+C/V, Suppr) ──────
with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    native_config(home)
    rc = ET.parse(home / ".config/openbox/rc.xml").getroot()
    contexts = {item.get("name"): item for item in rc.find(f"{OB}mouse")}
    desktop = contexts["Desktop"]
    ok(not any(bind.get("button") == "Right" for bind in desktop),
       "openbox laisse le clic droit à xfdesktop (Renommer, Supprimer, Copier sur un fichier)")
    ok(desktop.find(f"{OB}mousebind/{OB}action/{OB}desktop") is not None,
       "un clic donne le clavier au bureau (focus <desktop>yes</desktop>)")
    ok(contexts["Root"].findtext(f"{OB}mousebind/{OB}action/{OB}menu") == "root-menu", "menu Winlux gardé sur la racine")
    keys = {bind.get("key"): bind.findtext(f"{OB}action/{OB}command") for bind in rc.find(f"{OB}keyboard")}
    ok(keys.get("C-A-t") == "winlux-terminal", "Ctrl+Alt+T ouvre le terminal Winlux par openbox")
    uca = (home / ".config/Thunar/uca.xml").read_text()
    ok("Ouvrir dans le terminal" in uca and "Open Terminal Here</name>\n" not in uca.split("xml:lang")[0],
       "menu du bureau : « Ouvrir dans le terminal » en français")
    ok((home / ".local/share/Trash/files").is_dir() and (home / ".local/share/Trash/info").is_dir(),
       "Corbeille prête sur un profil neuf (pas « dossier introuvable »)")

with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    (home / ".config/openbox").mkdir(parents=True)
    old = (NATIVE / "config/openbox/rc.xml").read_text()
    old = old.split("<!-- Bureau :")[0] + '<context name="Desktop"><mousebind button="Right" action="Press"><action name="ShowMenu"><menu>root-menu</menu></action></mousebind></context>\n  </mouse>' + old.split("</mouse>", 1)[1]
    old = old.replace('    <keybind key="C-A-t">', '    <keybind key="X-unused">')
    (home / ".config/openbox/rc.xml").write_text(old)
    (home / ".config/Thunar").mkdir(parents=True)
    (home / ".config/Thunar/uca.xml").write_text("<actions>\n<action>\n\t<name>Open Terminal Here</name>\n</action>\n</actions>\n")
    native_config(home)
    rc = ET.parse(home / ".config/openbox/rc.xml").getroot()
    contexts = {item.get("name"): item for item in rc.find(f"{OB}mouse")}
    ok(not any(bind.get("button") == "Right" for bind in contexts["Desktop"]) and "Root" in contexts,
       "profil existant : ancien clic droit d'openbox sur le bureau migré")
    ok("Ouvrir dans le terminal" in (home / ".config/Thunar/uca.xml").read_text(), "profil existant : action Thunar traduite")

# ── Barre des tâches : icône nm-applet en double ───────────────────────────
with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    native_config(home)
    native_config(home)
    panel = (home / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").read_text()
    ok(panel.count('value="nm-applet"') == 1, "icône nm-applet masquée une seule fois (relances idempotentes)")

# ── Lanceurs de confiance (« Lanceur d'application non fiable ») ───────────
source = (SCRIPTS / "winlux-trust-shortcuts.py").read_text()
ok("metadata::xfce-exe-checksum" in source and "hashlib.sha256" in source,
   "winlux-trust-shortcuts pose la somme SHA-256 lue par xfdesktop 4.20")
pages_source = (SCRIPTS / "winlux-settings-pages.py").read_text()
ok("def forget_uninstalled" in pages_source and "timeout_add(1500, self.forget_uninstalled" in pages_source,
   "Paramètres > Programmes Windows : un programme désinstallé quitte la liste sans rouvrir la page")
programs_source = (ROOT / "testexe/src/shell/programs.rs").read_text()
ok('b"squirrel" => b"Squirrel"' in programs_source and 'b" application Windows"' in programs_source,
   "Applications installées : type Squirrel affiché, « 1 application Windows » au singulier")
menu_source = (SCRIPTS / "winlux-start-menu.py").read_text()
ok("metadata::xfce-exe-checksum" in menu_source, "raccourci créé depuis le menu Démarrer : somme de confiance posée")
ok("list_actions" in menu_source and "launch_action" in menu_source,
   "clic droit du menu Démarrer : actions de l'application (« Mettre à jour Claude Code »)")

# ── Menu Démarrer : nom du compte et « Retour » pendant une recherche ──────
menu = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="test_campaign")
entry = SimpleNamespace(pw_uid=1000, pw_gecos="Damien Test,,,", pw_name="damien")
with patch("os.getuid", return_value=1000), patch("pwd.getpwuid", return_value=entry):
    ok(menu["account_name"]() == "Damien Test", "menu Démarrer : nom complet du compte, pas « Winlux »")
entry = SimpleNamespace(pw_uid=1000, pw_gecos="", pw_name="damien")
with patch("os.getuid", return_value=1000), patch("pwd.getpwuid", return_value=entry):
    ok(menu["account_name"]() == "damien", "menu Démarrer : identifiant quand le nom complet est vide")
state = {"text": "final", "rendered": 0}
fake = SimpleNamespace(all_apps=False,
                       search=SimpleNamespace(get_text=lambda: state["text"], set_text=lambda text: state.update(text=text)),
                       render=lambda: state.update(rendered=state["rendered"] + 1))
menu["Start"].toggle_list(fake)
ok(state["text"] == "" and fake.all_apps is False and state["rendered"] == 1, "« Retour » efface la recherche en cours")
menu["Start"].toggle_list(fake)
ok(fake.all_apps is True, "« Toutes les applications » s'ouvre sans recherche")

# ── Terminal Winlux : coller ce qui a été copié (signalé par damien) ───────
with tempfile.TemporaryDirectory() as directory:
    log = Path(directory) / "xterm.log"
    xterm = fake_tool(Path(directory), "xterm", f'printf "%s\\n" "$@" > "{log}"')
    subprocess.run(["sh", str(ROOT / "integration/winlux-terminal")], env={**os.environ, "WINLUX_XTERM": str(xterm)}, check=True)
    arguments = log.read_text()
    for gesture, label in (("Shift Ctrl <Key>V: insert-selection(CLIPBOARD)", "Ctrl+Maj+V"),
                           ("Shift <Key>Insert: insert-selection(CLIPBOARD)", "Maj+Inser"),
                           ("<Btn3Down>: insert-selection(CLIPBOARD)", "clic droit"),
                           ("Shift Ctrl <Key>C: copy-selection(CLIPBOARD)", "Ctrl+Maj+C (copier)")):
        ok(gesture in arguments, f"terminal : {label} utilise le presse-papiers (CLIPBOARD)")

# ── su sous root verrouillé (signalé par damien sur la v0.2.17) ────────────
ok((ROOT / "integration/native/shell/su").is_file()
   and "install_su_wrapper()" in (ROOT / "scripts/winlux-update-apt.py").read_text(),
   "/usr/bin/su passe par sudo quand root est verrouillé (pas seulement dans le terminal)")
ok('("usr/bin/passwd", "root", "root", 0o4755)' in (ROOT / "scripts/winlux-update-apt.py").read_text(),
   "droits setuid de la base Ubuntu rétablis au démarrage (passwd, unix_chkpwd, shadow)")

# ── .exe qui ne démarre pas : message clair, pas de charabia ───────────────
with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    dialog = work / "dialog.log"
    fake_tool(work, "kdialog", f'printf "%s\\n" "$@" >> "{dialog}"')
    for tool in ("dirname", "basename", "mkdir", "tail", "grep", "sed", "head", "date", "cat", "printf"):
        found = subprocess.run(["sh", "-c", f"command -v {tool}"], capture_output=True, text=True).stdout.strip()
        if found.startswith("/"):
            (work / tool).symlink_to(found)
    runtime32 = fake_tool(work, "runtime32", 'case "$1" in --inspect) echo "programme Windows 32 bits (machine 0x014c) : seul le 64 bits est pris en charge pour l\'instant" >&2; exit 1 ;; esac; exit 1')
    exe = work / "setup.exe"
    exe.write_bytes(b"MZ")
    env = {"PATH": str(work), "DISPLAY": ":99", "HOME": str(work), "WINLUX_RUNTIME": str(runtime32), "WINLUX_DESKTOP_LAUNCH": "1"}
    subprocess.run(["/bin/sh", str(ROOT / "integration/winlux-open"), str(exe)], env=env)
    text = dialog.read_text()
    ok("programme Windows 32 bits" in text and "64 bits" in text and "Rien n’a été modifié" in text,
       ".exe 32 bits : explication en français et piste de la version 64 bits")
    dialog.unlink()
    runtime_dll = fake_tool(work, "runtime-dll", 'case "$1" in --inspect) echo gui; exit 0 ;; esac; '
                            'echo "winlux[error/loader]: chargement impossible : DLL introuvable : IMM32.dll (ni intégrée)" >&2; exit 1')
    subprocess.run(["/bin/sh", str(ROOT / "integration/winlux-open"), str(exe)], env={**env, "WINLUX_RUNTIME": str(runtime_dll)})
    text = dialog.read_text()
    ok("ne peut pas encore fonctionner avec Winlux" in text and "IMM32.dll" in text,
       "DLL manquante : nom du composant Windows et message compréhensible")
    ok("s’est arrêté (code" not in text.splitlines()[2], "DLL manquante : plus de « s'est arrêté (code 1) » brut en tête")

# ── Boutique : désinstallation sans trace ──────────────────────────────────
with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {"HOME": directory, "XDG_RUNTIME_DIR": directory}):
    store = runpy.run_path(str(SCRIPTS / "winlux-store.py"), run_name="test_campaign")
    calls = []
    apt = store["Apt"].__new__(store["Apt"])
    apt.apt = lambda arguments, progress: calls.append(arguments) or 0
    store["Apt"].remove(apt, "gnome-mines", None)
    ok(calls and calls[0][:2] == ["purge", "--autoremove"], "Boutique : désinstaller = purge --autoremove (réglages et dépendances partent)")
store_source = (SCRIPTS / "winlux-store.py").read_text()
ok(store_source.index("write_index(sources, apps, False)") < store_source.index('progress(100, tr(done'),
   "Boutique : index réécrit avant « done » (plus de « Obtenir » après installation)")
ok("contains_ci(reference, q)" in (ROOT / "testexe/src/shell/store.rs").read_text(),
   "Boutique : la recherche trouve aussi le nom du paquet (« mines » → Démineur)")

# ── Paramètres ─────────────────────────────────────────────────────────────
with patch.dict(os.environ, {"WINLUX_LANG": "fr"}):
    settings = runpy.run_path(str(SCRIPTS / "winlux-settings.py"), run_name="test_campaign")
about = settings["about_build"]("version=v0.2.13\ncommit=b0ee03dd41cef83\ntracked_sources=clean\nbuilt_at_utc=2026-11-01T21:30:12Z\nscope=system\n")
ok("Révision : b0ee03dd41" in about and "Compilée le : 01/11/2026" in about and "scope" not in about,
   "À propos : révision et date lisibles, sans clés brutes")
control = (SCRIPTS / "winlux-control.sh").read_text()
ok("Opération terminée." in control and "Operation terminee" not in control, "messages d'opération accentués")
settings_source = (SCRIPTS / "winlux-settings.py").read_text()
ok("page_opened" in settings_source, "« Opération terminée. » ne suit plus d'une page à l'autre")

# ── Image et session ───────────────────────────────────────────────────────
build = (SCRIPTS / "build-live-iso.sh").read_text()
for module, why in (("nls_utf8", "CIFS et noms UTF-8"), ("usb_storage", "clé USB → /dev/sdX"), ("uas", "disques USB rapides")):
    ok(f" {module} " in build, f"module {module} livré ({why})")
ok("killall -USR1 picom" in build, "live : picom remis à zéro quand le bureau est relancé (fenêtres noires)")
for script in ("winlux-native-session.sh", "winlux-user-session.sh"):
    ok("/usr/games" in (SCRIPTS / script).read_text(), f"{script} : /usr/games dans PATH (jeux visibles dans le menu)")
ok("pkill -USR1 -x picom" in (SCRIPTS / "winlux-native-session.sh").read_text(),
   "premier démarrage : Bienvenue repeinte après l'arrivée du bureau")
ok('"_WINLUX_MAXIMIZED = 1"; corner-radius = 0' in (ROOT / "integration/native/config/picom/picom.conf").read_text()
   and all("/usr/bin/winlux-maximized-watch" in (SCRIPTS / s).read_text() for s in ("winlux-native-session.sh", "winlux-user-session.sh")),
   "fenêtre agrandie : coins carrés, fond d'écran invisible aux coins (#582)")
installer = (SCRIPTS / "winlux-installer.py").read_text()
ok('"root/Bureau/winlux-installer.desktop"' in installer, "système installé : « Installer Winlux » retiré du bureau")
ok('"winlux-power", "now", "reboot"' in installer or '[power, "now", "reboot"]' in installer,
   "installateur : « Redémarrer maintenant » passe par winlux-power")
ok("def show_license_required" in installer and installer.count('"license_required":') == 6,
   "installateur : « Suivant » sans licence acceptée montre la case et un message (6 langues)")
tools = (SCRIPTS / "winlux-system-tools.py").read_text()
ok("/sys/class/ieee80211" in tools and "/sys/class/bluetooth" in tools,
   "réglages rapides : Wi-Fi et Bluetooth masqués sans matériel")
claude = (NATIVE / "claude-code/winlux-claude-code.py").read_text()
ok("La connexion Internet a été interrompue" in claude, "Claude Code : coupure réseau expliquée en français")
explorer = (ROOT / "testexe/src/shell/explorer.rs").read_text()
for marker, label in (("fn drop_target", "glisser-déposer sur un dossier ou un emplacement"),
                      ("fn restore_selection", "« Restaurer » dans la Corbeille"),
                      ("fn watch_drives", "« Ce PC » suit les clés USB"),
                      ("RENAME_ALL", "Ctrl+A en renommage"),
                      ("(0, true, 0x56) => 22", "Ctrl+V dans les champs de l'Explorateur")):
    ok(marker in explorer, f"Explorateur : {label}")

# ── Widgets Windows+W (#293, passage en VM) ────────────────────────────────
widgets = (SCRIPTS / "winlux-widgets.py").read_text()
ok("Gdk.SeatCapabilities.POINTER, True" in widgets,
   "widgets : saisie du pointeur owner_events=True (boutons, agenda et pense-bête répondent)")
ok("hidden_at" in widgets, "widgets : un second Windows+W ferme le volet au lieu de le rouvrir")
ok("for child in self.add_box.get_children():\n                child.show_all()" in widgets,
   "widgets : un widget retiré apparaît tout de suite dans « Ajouter des widgets »")
with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    native_config(home)
    rc = ET.parse(home / ".config/openbox/rc.xml").getroot()
    keys = {bind.get("key"): bind.findtext(f"{OB}action/{OB}command") for bind in rc.find(f"{OB}keyboard")}
    ok(keys.get("W-w") == "winlux-widgets", "Windows+W ouvre les widgets")

# ── Jeux (#294, passage en VM) ─────────────────────────────────────────────
games = runpy.run_path(str(NATIVE / "jeux/winlux-games.py"), run_name="test_campaign")
with tempfile.TemporaryDirectory() as directory:
    folder = Path(directory) / "Démo"
    folder.mkdir()
    exe = folder / "demo.exe"
    pe = bytearray(512)
    pe[0:2] = b"MZ"; pe[60:64] = (128).to_bytes(4, "little")
    pe[128:132] = b"PE\0\0"; pe[148:150] = (240).to_bytes(2, "little")
    pe[152:154] = (0x20B).to_bytes(2, "little")
    exe.write_bytes(bytes(pe))
    ok(games["best_exe"](folder) is None and games["best_exe"](folder, any_exe=True) is not None,
       "Jeux : un .exe sans import graphique (Unity, D3D à la demande) est retenu dans un dossier de jeux")
ok(games["summary"]({"exe": "/x.exe", "wide": True}, {"/x.exe": {"seconds": 0, "last": 1, "running": False}}) != "Jamais lancé",
   "Jeux : une partie de moins de 30 s n'affiche plus « Jamais lancé »")
games_source = (NATIVE / "jeux/winlux-games.py").read_text()
ok('Path("/proc", pid)' in games_source, "Jeux : un jeu quitté n'est plus « En cours » (bouton Jouer grisé)")
ok('WINLUX_GAME="1"' in games_source and 'var_os("WINLUX_GAME")' in (ROOT / "runtime/src/main.rs").read_text(),
   "Jeux : lancé depuis la bibliothèque, le temps de jeu est compté")
for script in ("winlux-personalize.py", "winlux-settings-pages.py", "winlux-native-session.sh", "winlux-user-session.sh"):
    text = (SCRIPTS / script).read_text()
    ok('"Adwaita-dark" if' not in text and "gtk_theme=Adwaita-dark" not in text,
       f"{script} : thème sombre par prefer-dark, pas « Adwaita-dark » (inexistant, rendu clair)")

# ── Petits plaisirs (#292, passage en VM) ──────────────────────────────────
native_session = (SCRIPTS / "winlux-native-session.sh").read_text()
for daemon in ("winlux-clipboard daemon", "winlux-alarms daemon", "winlux-ambiance daemon"):
    ok(daemon in native_session, f"session live : {daemon} lancé (Windows+V, alarmes, éclairage nocturne)")
clipboard = (SCRIPTS / "winlux-clipboard.py").read_text()
ok('label="📌"' not in clipboard and "user-trash-symbolic" in clipboard,
   "Windows+V : boutons Épingler/Supprimer en icônes du thème (pas d'emoji illisible sans police)")
taskview = (SCRIPTS / "winlux-taskview.py").read_text()
ok("self.set_app_paintable(True)" not in taskview, "vue des tâches : le voile couvre tout l'écran")
snap = (SCRIPTS / "winlux-snap.py").read_text()
ok(snap.count('"focus-out-event"') >= 2, "assistant d'ancrage : se ferme quand on passe à une autre fenêtre")

# ── Petits plaisirs (2) (#305, passage en VM) ──────────────────────────────
ok("apt_candidate(target[4:])" in (SCRIPTS / "winlux-store.py").read_text(),
   "Boutique : un paquet Ubuntu hors catalogue s'installe (police d'emojis, moteur de Win+Maj+T)")
ok('chmod 644 "$resolver"' in (SCRIPTS / "winlux-dhcp-hook.sh").read_text(),
   "DHCP : resolv.conf lisible par apt (_apt) après un renouvellement")

ok("Éditeur vérifié : Winlux" in (SCRIPTS / "winlux-uac.py").read_text(),
   "UAC : un outil de Winlux (Store, Paramètres) affiche un éditeur vérifié, pas « inconnu »")

ok('"LC_ALL": "C"' in (SCRIPTS / "winlux-store.py").read_text(),
   "Boutique : version candidate APT lue en anglais (session en français : « Candidat : »)")
for script in ("winlux-native-session.sh", "winlux-user-session.sh"):
    text = (SCRIPTS / script).read_text()
    ok(".local/share/themes/$gtk_theme" in text and "winlux-personalize theme" in text,
       f"{script} : thème du compte reconnu, thème absent remplacé par le thème Winlux")

lock = (SCRIPTS / "winlux-lock.py").read_text()
ok("def _screen_suffix" in lock and "0o1777" in lock,
   "verrouillage : un marqueur par écran (le second compte peut verrouiller et changer d'utilisateur)")
ok("Terminal libre" in (SCRIPTS / "winlux-sessions.sh").read_text(),
   "changer d'utilisateur : le second bureau évite le VT du premier")

# ── Wi-Fi sur vrai matériel (03/11/2026) : « non autorisé », pas de saisie de clé ──
wifi = (SCRIPTS / "winlux-wifi.py").read_text()
ok('"winlux-root") or "/usr/bin/winlux-root", "exec", "winlux-wifi"' in wifi,
   "Wi-Fi : le compte passe par le relais root (nmcli en utilisateur répondait « non autorisé »)")
ok("--key-stdin" in wifi and "0o600" in wifi, "Wi-Fi : clé lue sur l'entrée standard, profil système 0600")
ok('"winlux-wifi"' in (SCRIPTS / "winlux-root.py").read_text(), "relais root : winlux-wifi autorisé")
ok('["winlux-shell-app","settings","35"]' in (SCRIPTS / "winlux-popup.py").read_text(),
   "tuile Wi-Fi : la flèche ouvre la page Wi-Fi, plus « Outils Winlux » en entier")
ok("nm-connection-editor" not in (SCRIPTS / "winlux-system-tools.py").read_text(),
   "Outils Winlux : plus de liste de commandes nmcli, renvoi vers les pages Wi-Fi et Bluetooth")
ok((ROOT / "integration/native/network/50-winlux-network.rules").is_file(),
   "polkit : les comptes du groupe netdev pilotent NetworkManager")

# ── À propos > Votre PC (damien, 2026-11-03) ───────────────────────────────
about_pages = (SCRIPTS / "winlux-settings-pages.py").read_text()
hardware_page = (SCRIPTS / "winlux-hardware-page.py").read_text()
ok('9: "sections_about"' in about_pages and "def pc_section" in hardware_page,
   "À propos : section « Votre PC » sous la version et le noyau")
ok("lines.update(pc_status(devices))" in (SCRIPTS / "winlux-drivers.py").read_text(),
   "Votre PC : relevé de winlux-drivers (même détection que Matériel et pilotes)")
ok('"Votre PC": ("Your PC", "Su PC", "Ihr PC")' in hardware_page, "Votre PC : quatre langues")

# ── xdg-open ~/Téléchargements ouvrait « e:///home/…/T%C3%A9l… » (introuvable) ──
shell_app = SCRIPTS / "winlux-shell-app.sh"
for uri, expected in (("file:///home/damien/T%C3%A9l%C3%A9chargements", "Z:\\home\\damien\\Téléchargements"),
                      ("file:///tmp/Mes%20documents%20%231", "Z:\\tmp\\Mes documents #1")):
    resolved = subprocess.run(["sh", str(shell_app), "--chemin", uri], capture_output=True).stdout
    ok(resolved == expected.encode(), f"xdg-open sur un dossier : {uri} → {expected}")

# ── Support matériel des PC (audit de la v0.2.17, 2026-11-03) ──────────────
build = (SCRIPTS / "build-live-iso.sh").read_text()
ok("hardware-module-dirs.list" in build and "check-image-hardware.py" in build,
   "image : dossiers entiers de pilotes et contrôle des pilotes essentiels (911 modules sur 6 901 en v0.2.17)")
essentials = (SCRIPTS / "hardware-essentials.list").read_text()
ok("module pinctrl_sunrisepoint" in essentials and "module uhid" in essentials and "module binfmt_misc" in essentials,
   "pavés tactiles Intel 6e–9e gén., Bluetooth LE et ./programme.exe ne peuvent plus disparaître de l'image")
drivers_tool = (SCRIPTS / "winlux-drivers.py").read_text()
ok("def module_packages" in drivers_tool and "if apt_available(p)" in drivers_tool,
   "« Installer les pilotes complets » : linux-modules-extra n'est plus exigé (noyau 7.0)")

# ── Liens web sur le portable réel (v0.2.18, 2026-11-03) ───────────────────
# La connexion d'une application ouvrait les Paramètres > Navigateurs alors
# qu'un navigateur était installé (installé sans être « par défaut »).
with tempfile.TemporaryDirectory() as directory:
    base = Path(directory)
    firefox = base / "system/opt/winlux/browsers/firefox/root/usr/lib/firefox/firefox"
    firefox.parent.mkdir(parents=True)
    fake_tool(firefox.parent, "firefox", f'printf "%s\\n" "$@" > "{base}/ouvert"')
    fake_tool(base, "winlux-browsers", f'exec sh "{SCRIPTS}/winlux-browsers.sh" "$@"')
    env = {**os.environ, "WINLUX_BROWSER_ROOT": str(base / "system"), "WINLUX_BROWSER_CANDIDATES": "",
           "PATH": f"{base}:{os.environ['PATH']}"}
    env.pop("WINLUX_BROWSER", None)
    url = "https://example.invalid/oauth?code=1"
    subprocess.run(["sh", str(ROOT / "integration/winlux-browser"), url], env=env, check=True)
    ok((base / "ouvert").read_text().split() == [url] and not (base / "system/run/winlux/browser-chooser").exists(),
       "lien d'une application : ouvert dans le navigateur installé, sans renvoyer aux Paramètres")

# Retour du testeur sur #418 : lien gardé jamais ouvert après l'installation
# depuis Paramètres, Firefox en anglais sur un système en français.
settings = (SCRIPTS / "winlux-settings.py").read_text()
ok("open_pending_link()" in settings and '"Définir {name} par défaut"' in settings,
   "Paramètres ouvre le lien gardé dès le navigateur installé ; bouton « Définir … par défaut »")
ok("add_langpacks" in (SCRIPTS / "winlux-browsers.sh").read_text() and "firefox-l10n-{lang}" in (SCRIPTS / "prepare-browser-packages.py").read_text(),
   "Firefox installé avec ses paquets de langue fr, de, es")

# ── Ancrage à la souris (#460) ─────────────────────────────────────────────
shipped_rc = ET.parse(NATIVE / "config/openbox/rc.xml").getroot()
ok(shipped_rc.findtext(f"{OB}mouse/{OB}screenEdgeWarpTime") == "0",
   "glisser une fenêtre au bord l'ancre au lieu de changer de bureau virtuel")
ok(all("winlux-snap watch" in (SCRIPTS / name).read_text() for name in ("winlux-user-session.sh", "winlux-native-session.sh")),
   "ancrage à la souris lancé avec la session (compte et live)")

# ── Assistant d'ancrage (#585) ─────────────────────────────────────────────
snap_tool = runpy.run_path(str(SCRIPTS / "winlux-snap.py"), run_name="campaign_snap")
snap_area = (0, 0, 1366, 728)
ok(snap_tool["free_zones"](snap_tool["following_zones"]("quarter:tr"), snap_area, [snap_tool["zone_rect"]("half:left", snap_area)]) == ["quarter:br"],
   "assistant d'ancrage : après un coin, seule la place encore libre est proposée")
ok(snap_tool["card_grid"](683, 364, 1, 6)[0] >= 2, "assistant d'ancrage lisible dans un quart en 1366x768")

# ── Recherche du menu Démarrer (#461) ─────────────────────────────────────
for query, page in (("wifi", 35), ("comptes", 33), ("imprimante", 26), ("telephone", 37), ("accessibilite", 12)):
    ok([p for _, p in menu["settings_for"](query)][:1] == [page], f"recherche « {query} » : page {page} des Paramètres")

# ── Bureaux virtuels dans la barre (#459) ────────────────────────────────
with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    native_config(home)
    panel = ET.parse(home / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").getroot()
    order = [value.get("value") for value in panel.find(".//property[@name='plugin-ids']")]
    after = [panel.find(f".//property[@name='plugin-{number}']/property[@name='items']/value") for number in order[order.index('1') + 1:order.index('1') + 3]]
    ok([item.get("value").rsplit("/", 1)[-1] if item is not None else None for item in after] == ["winlux-search.desktop", "winlux-taskview.desktop"],
       "profil neuf : boutons Rechercher et Vue des tâches juste après Démarrer")
    ok(panel.find(".//property[@name='all-workspaces']").get("value") == "false",
       "profil neuf : la barre ne montre que les fenêtres du bureau en cours")
    rc = ET.parse(home / ".config/openbox/rc.xml").getroot()
    ok(rc.find(f".//{OB}keybind[@key='W-C-S-Right']") is not None,
       "Windows+Ctrl+Maj+→ emmène la fenêtre sur le bureau voisin")

# ── Sauvegarde des documents (damien, 2026-11-05) ─────────────────────────
backup_tool = (SCRIPTS / "winlux-backup.sh").read_text()
ok("PAGE_BACKUP = 38" in about_pages and '(_t("Comptes"), [33, 38])' in settings,
   "Paramètres › Comptes › Sauvegarde (page 38, native seulement)")
ok("staging()" in backup_tool and 'chgrp "$gid" "$d"' in backup_tool,
   "sauvegarde : état lisible par le compte sur le système installé (relais root, plus de fichiers 0600 root)")
ok("restore_file()" in backup_tool and "safe    :" in backup_tool and "setpriv --reuid" in backup_tool,
   "restauration « conserver les deux », copiée sous le compte et jamais en root")
ok("in_scope()" in backup_tool and "real_in_scope" in backup_tool,
   "sauvegarde : un compte ne protège que ses propres dossiers (ni /etc ni un lien vers ailleurs)")
with tempfile.TemporaryDirectory() as directory:
    base = Path(directory)
    (base / "etc").mkdir()
    (base / "proc").mkdir()
    (base / "proc/mounts").write_text("")
    (base / "etc/passwd").write_text("eve:x:1001:1001::/home/eve:/bin/sh\n")
    (base / "home/eve/Documents").mkdir(parents=True)
    env = {**os.environ, "WINLUX_CONTROL_ROOT": str(base)}
    refused = subprocess.run(["sh", str(SCRIPTS / "winlux-backup.sh"), "add", "eve", "/etc"], env=env, capture_output=True)
    ok(refused.returncode != 0, "sauvegarde : /etc refusé comme dossier d'un compte")
    refused = subprocess.run(["sh", str(SCRIPTS / "winlux-backup.sh"), "set", "eve", "DESTINATION", "/etc"], env=env, capture_output=True)
    ok(refused.returncode != 0, "sauvegarde : /etc refusé comme destination")

# ── Envoi automatique des plantages (#438) ───────────────────────────────
# crash.log est en mémoire vive : un plantage non signalé disparaissait au
# redémarrage. Il part maintenant au démarrage suivant, sans fenêtre.
startup = (SCRIPTS / "winlux-session-startup.py").read_text()
ok('"winlux-report","--background"' in startup and "STDERR_SILENCE" in startup,
   "ouverture de session : envoi des plantages en arrière-plan, sans sortie visible")
report_tool = (SCRIPTS / "winlux-report.py").read_text()
ok("def clock_ready" in report_tool and "time-sync.json" in report_tool,
   "envoi automatique : attend une horloge juste (piège TLS « not yet valid »)")
ok('"--collect"' in report_tool and "auto-report" in report_tool,
   "plantage gardé dans une file persistante dès qu'il se produit")
installer = (SCRIPTS / "winlux-installer.py").read_text()
ok('"crash_reports": True' in installer and "write_report_choice(etc" in installer
   and installer.count('"crash_reports":') >= 7,
   "installateur : case « envoyer les plantages » proposée cochée, traduite, écrite dans report.conf")
ok("report-token-set) winlux-report --set-token" in (SCRIPTS / "winlux-control.sh").read_text()
   and 'input=entry.get_text()' in (SCRIPTS / "winlux-settings-pages.py").read_text(),
   "jeton GitHub : passé à root par l'entrée standard, jamais en argument ni dans le compte")
ok("report-auto-on|report-auto-off" in (SCRIPTS / "winlux-control.sh").read_text()
   and "report-auto-on" in (SCRIPTS / "winlux-settings-pages.py").read_text(),
   "Paramètres > Signaler un problème : interrupteur de l'envoi automatique")
for language, word in (("fr", "anonymisé"), ("en", "anonymised"), ("de", "anonymisiert"), ("es", "anónimo")):
    privacy = (ROOT / f"site/content/{language}/privacy.md").read_text()
    ok(word in privacy and "Wi-Fi" in privacy.replace("WLAN", "Wi-Fi"),
       f"page confidentialité ({language}) : envoi automatique documenté")

# ── « Signaler un problème » : journaux de diagnostic (#458) ───────────────
# Pour le Wi-Fi (#377, #380, #398), le son (#383) ou l'heure (#419), il fallait
# redemander les journaux à la main.
report_tool = (SCRIPTS / "winlux-report.py").read_text()
ok(all(f'"{d}": {{"files"' in report_tool for d in ("network", "sound", "boot", "update"))
   and "/run/winlux/wpa-supplicant.log" in report_tool and '"LC_ALL": "C"' in report_tool,
   "rapport : journaux réseau, son, démarrage et mise à jour joignables, pactl en LC_ALL=C")
ok("d_empty" in report_tool and "queued_reports()" in report_tool,
   "rapport : journal vide signalé comme tel, plantages en attente listés")

# ── Windows + X au-dessus du bouton Démarrer (#574) ───────────────────────
ok("<position><x>0</x><y>-40</y></position>" in (ROOT / "integration/native/config/openbox/rc.xml").read_text()
   and 'winlux-run place-menu "$alignment"' in (SCRIPTS / "winlux-control.sh").read_text(),
   "Windows + X : menu au-dessus du bouton Démarrer, pas sous le pointeur, dans chaque compte")

print(f"Campagne v0.2.13 : {checks} régressions vérifiées.")
