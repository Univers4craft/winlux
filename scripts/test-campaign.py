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
    ok(contexts["Root"].findtext(f"{OB}mousebind/{OB}action/{OB}menu") == "winlux-desktop-menu",
       "menu du bureau traduit sur la racine (#787)")
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
    old = old.replace("<menu>winlux-desktop-menu</menu>", "<menu>root-menu</menu>")
    (home / ".config/openbox/rc.xml").write_text(old)
    (home / ".config/Thunar").mkdir(parents=True)
    (home / ".config/Thunar/uca.xml").write_text("<actions>\n<action>\n\t<name>Open Terminal Here</name>\n</action>\n</actions>\n")
    native_config(home)
    rc = ET.parse(home / ".config/openbox/rc.xml").getroot()
    contexts = {item.get("name"): item for item in rc.find(f"{OB}mouse")}
    ok(not any(bind.get("button") == "Right" for bind in contexts["Desktop"]) and "Root" in contexts,
       "profil existant : ancien clic droit d'openbox sur le bureau migré")
    keys = {bind.get("key"): bind.findtext(f"{OB}action/{OB}menu") for bind in rc.find(f"{OB}keyboard")}
    ok(keys.get("W-F10") == "winlux-desktop-menu"
       and contexts["Root"].findtext(f"{OB}mousebind/{OB}action/{OB}menu") == "winlux-desktop-menu",
       "profil existant : Windows + F10 et la racine ouvrent le menu du bureau traduit (#787)")
    for lang, word in (("en", "Open Explorer"), ("es", "Abrir Explorer"), ("de", "Explorer öffnen"), ("fr", "Ouvrir Explorer")):
        menu = subprocess.run([sys.executable, str(SCRIPTS / "winlux-run.py"), "desktop-menu"], capture_output=True, text=True,
                              env={**os.environ, "WINLUX_LANG": lang}).stdout
        ok(word in menu and "<openbox_pipe_menu>" in menu, f"menu du bureau en {lang} (#787)")
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
setup_run = (ROOT / "runtime/src/setup/run.rs").read_text()
ok("SYSTEM_TOOLS" in setup_run and "is_linux_program" in setup_run and "WINLUX_SETUP_DEPTH" in setup_run,
   "installateurs NSIS/Inno : programmes lancés (prérequis, ancien désinstalleur), jamais un programme Linux (#179)")
ok((ROOT / "scripts/test-setup-run.sh").is_file()
   and "scripts/test-setup-run.sh" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "test des programmes lancés par un installateur NSIS en CI (#179)")
setup_mod = (ROOT / "runtime/src/setup/mod.rs").read_text()
ok('format!("{root}\\\\{sub}")\n' in setup_mod
   and "Software\\\\DemoApp\\\\Réglages" in (ROOT / "scripts/test-setup-run.sh").read_text(),
   "installateurs NSIS/Inno : les noms de clés de registre gardent leur casse (#830)")
msi_actions = (ROOT / "runtime/src/msi/actions.rs").read_text()
ok("Phase::Uninstall" in msi_actions and "ServiceInstall" in msi_actions and "setup::run" in msi_actions,
   "msiexec : actions personnalisées EXE (installation et désinstallation) et services (#833)")
ok("winlux-setup-wizard" in (ROOT / "integration/winlux-open").read_text()
   and "winlux-setup-wizard.py:winlux-setup-wizard" in (SCRIPTS / "stage-desktop-components.sh").read_text()
   and "scripts/test-setup-wizard.sh" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "assistant d'installation au double-clic (.msi, NSIS, Inno) en quatre langues, testé en CI (#833)")
msi_session = (ROOT / "runtime/src/msi/session.rs").read_text()
ok("MsiGetPropertyW" in msi_session and "MsiProcessMessage" in msi_session
   and "--msi-dll-action" in (ROOT / "runtime/src/main.rs").read_text()
   and "i686-w64-mingw32-gcc" in (SCRIPTS / "test-msi-actions.sh").read_text(),
   "msiexec : actions personnalisées DLL x86 et x64 (types 1 et 17) avec session msi.dll (#833)")
msi_views = (ROOT / "runtime/src/msi/session_db.rs").read_text()
ok("MsiDatabaseOpenViewW" in msi_views and "MsiViewFetch" in msi_views
   and "MsiDoActionW" in (ROOT / "scripts/test-msi-actions.sh").read_text(),
   "msiexec : une action DLL lit la base du paquet (vues SQL) et planifie une action différée (#833)")
ok("fn msi_view_modify" in msi_views and "MsiRecordReadStream" in (ROOT / "runtime/src/msi/session.rs").read_text()
   and "MSIMODIFY_INSERT_TEMPORARY" in (ROOT / "scripts/test-msi-actions.sh").read_text(),
   "msiexec : une action DLL écrit dans la base (MsiViewModify) et lit les flux de la table Binary (#833)")
msi_script = (ROOT / "runtime/src/msi/script/mod.rs").read_text()
ok("--msi-script-action" in (ROOT / "runtime/src/main.rs").read_text()
   and "fn status_code" in msi_script and (ROOT / "runtime/src/msi/script/vbs.rs").exists()
   and (ROOT / "runtime/src/msi/script/js.rs").exists()
   and "scripts/test-msi-scripts.sh" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "msiexec : actions personnalisées VBScript et JScript (types 5, 6, 21, 22, 37, 38, 53, 54) avec l'objet Session (#973)")
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
                       render=lambda: state.update(rendered=state["rendered"] + 1),
                       scroll_top=lambda: state.update(top=state.get("top", 0) + 1))
menu["Start"].toggle_list(fake)
ok(state["text"] == "" and fake.all_apps is False and state["rendered"] == 1, "« Retour » efface la recherche en cours")
menu["Start"].toggle_list(fake)
ok(fake.all_apps is True and state.get("top") == 2, "« Toutes les applications » s'ouvre sans recherche, liste en haut")

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
# #886/#890/#891 : icônes Prisme par type, Terminal au trait, thème suivi en direct.
for marker, label in (("Kind::Spreadsheet", "tableur vert pour .ods/.xlsx (#890)"),
                      ("fn windows_program", "pastille Windows réservée aux .exe/.msi (#890)"),
                      ("Kind::Script", "scripts shell à part des programmes (#890)")):
    ok(marker in explorer, f"Explorateur : {label}")
ok("Glyph::Terminal => Icon::Terminal" in (ROOT / "testexe/src/shell/explorer_icons.rs").read_text(),
   "Explorateur : icône Terminal Prisme au trait (#886)")
ok("fn native_frameless" in (ROOT / "testexe/src/shell/wm.rs").read_text()
   and "fn begin_move_resize" in (ROOT / "runtime/src/dlls/gui.rs").read_text(),
   "Explorateur : onglets dans la barre de titre, fenêtre sans cadre déplaçable (#888)")
ok('b" GB", b" TB"' in explorer, "Explorateur : unités et dates dans la langue de la session (#916)")
ok("gfx::theme_animating()" in (ROOT / "testexe/src/bin/gui_demo.rs").read_text(),
   "Explorateur : passage clair/sombre appliqué à la fenêtre ouverte (#891)")

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

# ── PC de damien, v0.2.19 (07/11/2026) : Bluetooth, OpenGL, accessibilité ──
ok('GROUP="netdev"' in (ROOT / "integration/native/udev/60-winlux-rfkill.rules").read_text(),
   "Bluetooth : /dev/rfkill ouvert au groupe netdev, blueman-applet reste l'agent d'appairage (#604)")
desktop_packages = (SCRIPTS / "prepare-desktop-packages.py").read_text()
ok('"rfkill"' in desktop_packages and '"mesa-utils"' in desktop_packages and '"at-spi2-core"' in desktop_packages,
   "image : rfkill (#604), glxinfo pour la ligne OpenGL du rapport (#610), bus org.a11y.Bus (#615)")
ok('"-stream_name"' in (ROOT / "runtime/src/dlls/winlux_media.rs").read_text()
   and '"Lecteur multimédia"' in (ROOT / "runtime/src/audio/backend.rs").read_text(),
   "mélangeur : le son du lecteur s'appelle « Lecteur multimédia », plus « Lavf… » de ffmpeg (#1010)")
ok("WinluxMediaControl(SESSION, 2, volume)" in (ROOT / "testexe/src/shell/mediaplayer.rs").read_text(),
   "lecteur : pause et volume sans fermer le flux audio, plus de trou ni de clic (#1013)")

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
ok(snap_tool["group_members"]("a", {"a": ("g", "half:left"), "b": ("g", "half:right")}) == ["a", "b"],
   "groupe d'ancrage : deux fenêtres ancrées ensemble se restaurent ensemble (#597)")

# ── Recherche du menu Démarrer (#461) ─────────────────────────────────────
for query, page in (("wifi", 35), ("comptes", 33), ("imprimante", 26), ("telephone", 37), ("accessibilite", 12)):
    ok([p for _, p in menu["settings_for"](query)][:1] == [page], f"recherche « {query} » : page {page} des Paramètres")

# ── Bureaux virtuels dans la barre (#459) ────────────────────────────────
with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    native_config(home)
    panel = ET.parse(home / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").getroot()
    order = [value.get("value") for value in panel.find(".//property[@name='plugin-ids']")]
    def button(number):
        """Lanceur (fichier .desktop) ou genmon (commande) d'un greffon."""
        item = panel.find(f".//property[@name='plugin-{number}']/property[@name='items']/value")
        command = panel.find(f".//property[@name='plugin-{number}']/property[@name='command']")
        return item.get("value").rsplit("/", 1)[-1] if item is not None else command.get("value") if command is not None else None
    ok([button(number) for number in order[order.index('1') + 1:order.index('1') + 3]] == ["/usr/bin/winlux-tray search", "winlux-taskview.desktop"],
       "profil neuf : zone Rechercher (#703) et Vue des tâches juste après Démarrer")
    # Barre redessinée (#703) : réseau, son et batterie en un bouton, l'heure avant la cloche.
    ok([button(number) for number in order[order.index('10') + 1:order.index('10') + 4]]
       == ["/usr/bin/winlux-tray status", "/usr/bin/winlux-clock", "/usr/bin/winlux-tray notifications"],
       "profil neuf : état groupé, heure puis cloche (#703)")
    ok(panel.find(".//property[@name='all-workspaces']").get("value") == "false",
       "profil neuf : la barre ne montre que les fenêtres du bureau en cours")
    rc = ET.parse(home / ".config/openbox/rc.xml").getroot()
    ok(rc.find(f".//{OB}keybind[@key='W-C-S-Right']") is not None,
       "Windows+Ctrl+Maj+→ emmène la fenêtre sur le bureau voisin")

# ── Sauvegarde des documents (damien, 2026-11-05) ─────────────────────────
backup_tool = (SCRIPTS / "winlux-backup.sh").read_text()
ok("PAGE_BACKUP = 38" in about_pages and '(_t("Comptes"), [33, 38' in settings,
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
# Relier Winlux à GitHub (#1049 à #1052) : rien ne part sans confirmation
# « quoi / où » (Annuler par défaut), aucun journal sans case cochée,
# déconnexion qui révoque vraiment le jeton, suivi de ses signalements.
ok("def confirm_send" in report_tool and "set_default_response(Gtk.ResponseType.CANCEL)" in report_tool
   and "check.set_active(False)" in report_tool and '"network" in self.attach' in report_tool,
   "rapport : confirmation de ce qui part et où, journaux joints seulement sur accord")
ok("/credentials/revoke" in report_tool and '"--account-disconnect"' in (SCRIPTS / "winlux-settings-pages.py").read_text(),
   "compte GitHub : « Se déconnecter » révoque le jeton chez GitHub")
ok("def refresh_reports" in report_tool and '"--mine"' in (SCRIPTS / "winlux-settings-pages.py").read_text()
   and "def send_queue_gui" in report_tool,
   "compte GitHub : « Mes signalements » et envoi des plantages en attente avec son compte")

# ── Barre centrée : Démarrer immobile au démarrage (#602) ─────────────────
tray_source = (SCRIPTS / "winlux-tray.py").read_text()
ok("def _settled_balance" in tray_source and "taskbar-balance" in tray_source
   and "test-taskbar-boot-centering.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "barre centrée : contrepoids mesuré une fois les icônes de droite en place, valeur précédente reprise")
ok("test-taskbar-live-align.py" in (ROOT / ".github/workflows/ci.yml").read_text()
   and "*' 13 '*) ;;" not in (SCRIPTS / "winlux-control.sh").read_text(),
   "barre : changement d'alignement en direct suivi d'une relance de la barre (#651)")
ok("def scroll_top" in (SCRIPTS / "winlux-start-menu.py").read_text()
   and "test-start-menu-reset.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "menu Démarrer : chaque ouverture repart en haut de la vue Épinglé (#673)")
ok("def render_side" in (SCRIPTS / "winlux-start-menu.py").read_text()
   and "def start_menu_settings" in (SCRIPTS / "winlux-settings-pages.py").read_text()
   and "test-start-for-you.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "menu Démarrer à deux volets : « Pour vous » calculé sur le PC, coupable dans Paramètres, recentré (#685)")

# ── Windows + X au-dessus du bouton Démarrer (#574) ───────────────────────
ok("<position><x>0</x><y>-40</y></position>" in (ROOT / "integration/native/config/openbox/rc.xml").read_text()
   and 'winlux-run place-menu "$alignment"' in (SCRIPTS / "winlux-control.sh").read_text(),
   "Windows + X : menu au-dessus du bouton Démarrer, pas sous le pointeur, dans chaque compte")

# ── Touche Windows, recherche, volume et notifications (#605–#607, #620, #624) ──
shipped_keys = {binding.get("key"): binding for binding in shipped_rc.find(f"{OB}keyboard")}
ok("Super_L" not in shipped_keys and all("/usr/bin/winlux-super-key" in (SCRIPTS / name).read_text()
                                         for name in ("winlux-user-session.sh", "winlux-native-session.sh")),
   "touche Windows seule : Démarrer par winlux-super-key, pas par une liaison qui volerait Windows+Gauche")
ok("found_in(query, name)" in (SCRIPTS / "winlux-start-menu.py").read_text(),
   "recherche de fichiers par début de mot, triée par pertinence puis date")
ok(all(key in shipped_keys for key in ("XF86AudioRaiseVolume", "XF86AudioLowerVolume", "XF86AudioMute")),
   "touches de volume du clavier liées à winlux-volume")
ok("def toast_position" in (SCRIPTS / "winlux-notifications.py").read_text(),
   "bulle de notification en bas à droite, au-dessus de la barre des tâches")

# ── Icônes Prisme claires et sombres (#616) ─────────────────────────────
icons = NATIVE / "icons"
ok((icons / "Winlux/index.theme").read_text().count("scalable/apps") >= 2
   and (icons / "Winlux/scalable/apps/winlux.svg").is_file()
   and "Inherits=Winlux," in (icons / "Winlux-Sombre/index.theme").read_text(),
   "icônes : applications gardées dans Winlux, Winlux-Sombre hérite de Winlux")
ok("for icon_theme in Winlux Winlux-Sombre" in (SCRIPTS / "stage-desktop-components.sh").read_text()
   and "gtk-update-icon-cache" in (SCRIPTS / "stage-apt-root.py").read_text()
   and "usr/share/icons/Winlux-Sombre/" in (SCRIPTS / "stage-apt-root.py").read_text(),
   "icônes : les deux thèmes dans winlux-desktop, cache GTK en postinst")
ok(all("icon_wanted=Winlux-Sombre" in (SCRIPTS / name).read_text() for name in ("winlux-user-session.sh", "winlux-native-session.sh"))
   and '"/Net/IconThemeName","Winlux-Sombre" if dark else "Winlux"' in (SCRIPTS / "winlux-personalize.py").read_text(),
   "icônes : Winlux-Sombre par défaut, suit le mode clair ou sombre de Personnalisation")

# ── Paramètres : « Mise à jour OS » et « Mise à jour application » (#637) ──
ok('_t("Mise à jour OS")' in (SCRIPTS / "winlux-settings.py").read_text()
   and '_t("Mise à jour application")' in (SCRIPTS / "winlux-settings.py").read_text()
   and (SCRIPTS / "test-update-page-names.py").exists(),
   "Paramètres : pages 7 et 17 nommées Mise à jour OS / Mise à jour application (4 langues)")

# #633 : journal et état propres à chaque compte, jamais en 0666 dans /run/winlux.
iso_init = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
ok("chmod 1777 /run/winlux/journal" not in iso_init and "echo 2 >/proc/sys/fs/protected_regular" in iso_init
   and "journal-$journal_uid" in iso_init,
   "journal : un dossier 0700 par compte, protected_regular=2")

# #721 / #722 : plus de « Gestionnaire de fichiers » en double, page Applications installées.
ok('"xfce4-file-manager.desktop"' in (SCRIPTS / "winlux-start-menu.py").read_text()
   and (SCRIPTS / "winlux-installed-apps.py").exists() and (SCRIPTS / "test-installed-apps.py").exists()
   and '_t("Sauvegarde"), _t("Applications installées")' in (SCRIPTS / "winlux-settings.py").read_text(),
   "menu Démarrer sans « Gestionnaire de fichiers » ; Paramètres > Applications installées (page 39)")

# #735 : XScreenSaver (dpmsEnabled: False par défaut) coupait le DPMS au premier
# économiseur ; la veille de l'écran ne revenait plus de la session.
with tempfile.TemporaryDirectory() as directory:
    fresh, old = Path(directory) / "neuf", Path(directory) / "ancien"
    fresh.mkdir(); old.mkdir()
    (old / ".xscreensaver").write_text("timeout:        0:10:00\nmode:           one\n")
    native_config(fresh); native_config(old)
    ok(all("dpmsEnabled:    True" in (home / ".xscreensaver").read_text() for home in (fresh, old))
       and (SCRIPTS / "test-ecran-veille-retour.py").exists(),
       "veille de l'écran : XScreenSaver garde le DPMS actif (compte neuf et compte existant)")

# #659 : code PIN et empreinte à la connexion, vérifiés par le relais root.
signin_tool = (SCRIPTS / "winlux-signin.py").read_text()
ok("hashlib.scrypt" in signin_tool and "MAX_PIN_TRIES = 5" in signin_tool
   and "def signin_prepare" in (SCRIPTS / "winlux-root.py").read_text()
   and "def submit_pin" in (SCRIPTS / "winlux-lock.py").read_text()
   and "def sections_signin" in (SCRIPTS / "winlux-settings-pages.py").read_text()
   and all((SCRIPTS / name).exists() for name in ("test-signin.py", "test-lock-signin.py", "test-signin-page.py")),
   "options de connexion : code PIN (scrypt, 5 essais) et empreinte par le relais, écran et Paramètres testés")

# #236 : DuplicateHandle et ReadProcessMemory entre processus (Chromium, Electron).
broker_source = (ROOT / "runtime/src/dlls/handle_broker.rs").read_text(encoding="utf-8")
ok("SCM_RIGHTS" in broker_source and "WINLUX_GUICHET_HANDLES" in broker_source
   and "stop_if_suspended" in (ROOT / "runtime/src/main.rs").read_text()
   and (SCRIPTS / "test-duplication.sh").exists(),
   "Electron : handles copiés entre processus, enfant suspendu une fois chargé")
permission_tool = (SCRIPTS / "winlux-permission.py").read_text(encoding="utf-8")
ok('"sandbox"' in permission_tool and "Lancer « {app} » sans son isolation ?" in permission_tool
   and '"sandbox": "Lancer sans isolation (Chromium, Electron)"' in (SCRIPTS / "winlux-settings.py").read_text(encoding="utf-8"),
   "Electron : bac à sable impossible → proposition claire, choix visible dans Paramètres")
ok((SCRIPTS / "winlux-webview.py").exists() and (SCRIPTS / "test-webview2.sh").exists()
   and "winlux-webview" in (SCRIPTS / "stage-desktop-components.sh").read_text(encoding="utf-8")
   and '"webview2loader"' in (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8"),
   "WebView2 : WebView2Loader.dll maison, moteur WebKitGTK livré à la demande")
# #662 : dictée vocale (Windows + H) et sous-titres en direct (Windows + Ctrl + L),
# modèle local vérifié par empreinte ; un compte existant reçoit les deux raccourcis.
with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    native_config(home)
    rc = home / ".config/openbox/rc.xml"
    text = rc.read_text()
    ok("winlux-speech dictate" in text and "winlux-speech captions" in text, "compte neuf : Windows + H et Windows + Ctrl + L")
    tree = ET.parse(rc)
    keyboard = tree.getroot().find(f"{OB}keyboard")
    for binding in [b for b in keyboard if b.get("key") in ("W-h", "W-C-l")]: keyboard.remove(binding)
    tree.write(rc)
    (rc.parent / "winlux-raccourcis-proposes.txt").write_text("W-h\nW-C-l\n")
    native_config(home)
    keys = {b.get("key") for b in ET.parse(rc).getroot().iter(f"{OB}keybind")}
    ok({"W-h", "W-C-l"} <= keys, "compte existant : la dictée et les sous-titres arrivent avec la mise à jour")
speech = (SCRIPTS / "winlux-speech.py").read_text()
ok('"sha256": "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898"' in speech
   and "digest.hexdigest() != info[\"sha256\"]" in speech and (SCRIPTS / "test-speech.py").exists(),
   "voix : modèle Whisper base téléchargé à la demande, empreinte SHA-256 vérifiée, rien dans l'ISO")
pages = (SCRIPTS / "winlux-settings-pages.py").read_text()
ok("if not mic and not output: return" in pages and '"LC_ALL": "C"' not in speech and 'LC_ALL="C"' in speech,
   "voix : section masquée sans micro ni sortie audio ; pactl et parec en locale C")
# #660 : pages VPN (40) et Point d'accès sans fil mobile (41), travail root
# par le relais, état dans la zone de notification et les Réglages rapides.
root_relay = (SCRIPTS / "winlux-root.py").read_text()
ok((SCRIPTS / "winlux-vpn.py").exists() and (SCRIPTS / "winlux-hotspot.py").exists()
   and (SCRIPTS / "test-vpn-hotspot.py").exists()
   and 'if tool == "winlux-vpn":' in root_relay and 'if tool == "winlux-hotspot":' in root_relay
   and '_t("VPN"), _t("Point d’accès sans fil mobile")' in (SCRIPTS / "winlux-settings.py").read_text()
   and "def tunnels():" in (SCRIPTS / "winlux-tray.py").read_text()
   and '"vpn": side_available("winlux-vpn")' in (SCRIPTS / "winlux-popup.py").read_text(),
   "Paramètres > VPN et Point d'accès sans fil mobile, relais root, zone de notification, Réglages rapides")
# #792 : interrupteurs de Paramètres à la couleur d'accent, pas au bleu d'Adwaita.
ok("switch:checked {{ background: {accent}" in (SCRIPTS / "winlux-settings.py").read_text(),
   "Paramètres : interrupteurs activés à la couleur d'accent")
# #661 : contrôle parental. Le compte surveillé ne lit ni ne modifie ses
# réglages par le relais root, et ne déverrouille pas hors du temps permis.
parental = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-parental.py")], capture_output=True, text=True,
                          timeout=600)
ok(parental.returncode == 0 and "bob ne modifie rien : disable bob" in parental.stdout
   and "temps écoulé : bob ne déverrouille pas sa session" in parental.stdout,
   "contrôle parental : le compte surveillé ne lève pas sa propre restriction")
# #815 / #816 : copie renommée d'une application bloquée, VPN ou proxy hors du DNS filtrant.
ok(parental.returncode == 0 and "copie renommée sans VERSIONINFO fermée par son empreinte (#815)" in parental.stdout
   and "copie renommée d'un .exe bloqué fermée (nom d'origine, #815)" in parental.stdout
   and "adresse du site ouverte au compte avant la réponse (#816)" in parental.stdout
   and "seules les adresses données par le filtre, #816" in parental.stdout,
   "contrôle parental : copie renommée reconnue, VPN et proxy hors du filtre refusés")
# #811 : « Changer d'utilisateur » ne répond plus « réussi » quand la session ne s'ouvre pas.
ok("def run_waited" in (SCRIPTS / "winlux-root.py").read_text() and "bascule ratée" in (SCRIPTS / "test-root.py").read_text(),
   "relais root : vrai code de retour malgré SIGCHLD ignoré")
# #237 : anciens jeux. DirectDraw, Direct3D 8 et MIDI existent, jeux32/jeux64 les testent
# en x86 et en x64 ; #777 : rappels x86 de timeSetEvent et waveOut convertis.
runtime_dlls = ROOT / "runtime/src/dlls"
ok((runtime_dlls / "ddraw.rs").exists() and (runtime_dlls / "winmm_midi.rs").exists()
   and (runtime_dlls / "d3d8.rs").exists()
   and (runtime_dlls / "d3dx9.rs").exists() and (runtime_dlls / "dsound_x86.rs").exists()
   and (runtime_dlls / "quartz.rs").exists() and (runtime_dlls / "dplayx.rs").exists()
   and (runtime_dlls / "d3dx9_asm.rs").exists() and (runtime_dlls / "d3dx9_shader.rs").exists()
   and (runtime_dlls / "d3dx9_mesh.rs").exists()
   and (runtime_dlls / "d3dx9_hlsl.rs").exists() and (runtime_dlls / "d3dx9_effect.rs").exists()
   and (runtime_dlls / "dpnet.rs").exists()
   and "DirectPlay 4 et 8" in (SCRIPTS / "test-wow32.sh").read_text()
   and (ROOT / "runtime/src/audio/synth.rs").exists()
   and "jeux32" in (SCRIPTS / "test-wow32.sh").read_text()
   and "wow32::callback64" in (runtime_dlls / "apps_win32.rs").read_text()
   and "wow32::callback64" in (runtime_dlls / "winmm_waveout.rs").read_text(),
   "anciens jeux : DirectDraw, Direct3D 8, D3DX9, D3D9/DirectSound 32 bits, DirectShow, DirectPlay 4 et 8, synthétiseur MIDI, rappels x86")
# #237 : entrée MIDI sur les claviers branchés, testée sans matériel par jeux32/jeux64.
ok((runtime_dlls / "winmm_midi_in.rs").exists()
   and "/dev/snd" in (runtime_dlls / "winmm_midi_in.rs").read_text()
   and "WINLUX_MIDI_IN_DIR" in (SCRIPTS / "test-wow32.sh").read_text()
   and "midi_in_tests" in (ROOT / "testc/wow32/jeux32.c").read_text(),
   "entrée MIDI : midiIn* lit /dev/snd/midiC*D* (statut courant, temps réel, système exclusif)")

# Page Mise à jour OS refaite (#875 à #882) : plus de rapport brut ni de
# boutons activer/désactiver, une carte d'état, pause, heures, historique, notes.
update_page = (SCRIPTS / "winlux-update-page.py").read_text()
settings_source = (SCRIPTS / "winlux-settings.py").read_text()
ok('"update-auto-on"), (_t("Désactiver"), "update-auto-off")' not in settings_source
   and "Redémarrer après installation" not in settings_source
   and "UpdatePanel(self" in settings_source
   and all(word in update_page for word in ("update-pause-", "update-resume", "update-hours-", "load_history", "notes_markup", "def notify"))
   and (SCRIPTS / "test-update-page.py").exists(),
   "page Mise à jour OS : carte d'état, pause, heures d'activité, historique, notes, notification (#875-#882)")

# Écran et Alimentation en menus (#895) ; Réseau, Comptes et Personnalisation
# refaits (#898) : plus d'interface réseau brute, carte du compte, aperçu.
pages_source = (SCRIPTS / "winlux-settings-pages.py").read_text()
ok("self.power_controls(choices)" in settings_source and 'f"power-set-' in settings_source
   and '(_t("Connexion"), "network")' not in settings_source
   and "def network_summary" in pages_source and "def desktop_preview" in pages_source
   and 'self.hero("user", name, detail' in pages_source
   and (SCRIPTS / "test-settings-redesign.py").exists(),
   "Paramètres : menus Écran/Alimentation, Réseau, Comptes et Personnalisation refaits (#895, #898)")

# #847 : WPF dessine son texte par IDWriteGlyphRunAnalysis et choisit le thème
# Aero2 (styles visuels actifs), sans thémer les fenêtres Win32 classiques.
uxtheme = (runtime_dlls / "uxtheme.rs").read_text()
ok("create_glyph_run_analysis" in (runtime_dlls / "dwrite.rs").read_text()
   and (runtime_dlls / "dwrite_glyphs.rs").exists()
   and "fn IsThemeActive() -> i32 {\n    1\n}" in uxtheme
   and "fn IsAppThemed() -> i32 {\n    0\n}" in uxtheme
   and "#DDDDDD" in (SCRIPTS / "test-wpf-bonjour.sh").read_text(),
   "WPF : texte DirectWrite et thème Aero2, rendu du bouton vérifié par test-wpf-bonjour.sh")

# #931 : rendu matériel de WPF par Direct3D 9 : appareil du modèle récent
# (CANSHARERESOURCE), shaders vs/ps 2.0 générés compilés par D3DCompile,
# glyphes posés par UpdateSurface, couleur du texte par BLENDFACTOR ;
# test-wpf-bonjour.sh vérifie le niveau de rendu 2 et les deux rendus.
d3d9_src = (runtime_dlls / "d3d9.rs").read_text()
ok("0xE002_0000" in d3d9_src
   and "fn copy_rect(" in d3d9_src
   and "(g.BlendColor)(" in d3d9_src
   and "fn is_d3d9_profile(" in (runtime_dlls / "d3dcompiler.rs").read_text()
   and "fn struct_uniform(" in (runtime_dlls / "d3dx9_hlsl.rs").read_text()
   and "for passe in materiel logiciel" in (SCRIPTS / "test-wpf-bonjour.sh").read_text(),
   "WPF : rendu matériel Direct3D 9 (niveau 2) et rendu logiciel vérifiés par test-wpf-bonjour.sh")

# #910-#913 : Assistant IA. Une fenêtre d'activation par assistant, llama.cpp, Qwen et Codex
# à des révisions figées avec leur empreinte, connexion sans navigateur sur ce
# PC (lien et code pour un autre appareil), claude.exe jamais lancé dans un dossier personnel.
assistant_source = (SCRIPTS / "winlux-assistant.py").read_text()
ok("assistant_setups" in (SCRIPTS / "winlux-settings-pages.py").read_text()
   and "org.winlux.AssistantSetup." in assistant_source
   and "LLAMA_ENGINES = {" in assistant_source and '"revision": "' in assistant_source
   and 'CODEX_VERSION = "' in assistant_source and "releases/latest" not in assistant_source
   and "def has_browser" in assistant_source and '"--device-auth"' in assistant_source
   and "cwd=work_dir()" in assistant_source and "cwd=str(HOME)" not in assistant_source,
   "Assistant IA : trois activations possibles, IA locale téléchargeable, connexion sans navigateur, sans question de fichiers")
# #928 : IA locale choisie selon le PC (mémoire, carte graphique Vulkan), moteur
# arrêté deux minutes après la dernière question, ressources dites.
ok("def pc_plan" in assistant_source and '"vulkan": (f"llama-{LLAMA_TAG}-bin-ubuntu-vulkan-x64.tar.gz"' in assistant_source
   and "LOCAL_IDLE_SECONDS = 120" in assistant_source and "def idle_stop" in assistant_source
   and "def memory(self)" in assistant_source and "def plan_text" in assistant_source,
   "IA locale : modèle adapté au PC, zéro ressource sans question, ressources affichées")
# #997 : claude.exe (300 Mo) tué faute de mémoire : le .exe est projeté, pas copié,
# la copie de main.rs est libérée avant le chargement, grande section projetée du disque.
loader_source = (ROOT / "runtime/src/loader.rs").read_text()
main_source = (ROOT / "runtime/src/main.rs").read_text()
ok("struct ImageFile" in loader_source and "fn map_pages" in loader_source
   and "let file = ImageFile::open(path)" in loader_source and "drop(source);" in main_source
   and "loader::ImageFile::open(&exe)" in main_source
   and "image_file_is_mapped_and_sections_are_private_copies" in loader_source
   and "def login_failure" in assistant_source
   and "crate::loader::ImageFile::open(path)" in (ROOT / "runtime/src/setup/mod.rs").read_text()
   and "crate::loader::ImageFile::open(path)" in (ROOT / "runtime/src/setup/burn.rs").read_text()
   and "scripts/test-image-memory.py" in (ROOT / ".github/workflows/ci.yml").read_text()
   and (ROOT / "docs/compat/memoire-au-lancement.md").exists(),
   "claude.exe : image projetée sans copie du fichier, connexion tuée faute de mémoire dite clairement (#997)")
# #1019 : un simple nom de DLL (LoadLibraryExW + SEARCH_SYSTEM32, CRT MSVC) n'est
# plus un chemin relatif au dossier courant : claude.exe lancé du Bureau attendait
# la demande « fichiers personnels » avant son main.
ok("let is_path = value.contains(" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
   and (ROOT / "testexe/src/bin/loadlib_search.rs").exists()
   and "test-loadlibrary-personal-cwd.sh" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "LoadLibraryExW d'une DLL système depuis le Bureau sans demande d'autorisation (#1019)")
# #1033-#1036 : IA locale 1,5B en VM. Pas de réglage pour « Bonjour », pas de dossier
# que la question ne nomme pas, Programmes Windows toujours proposé pour un .exe,
# bouton par défaut mis en évidence dans « Arrêter l'activation ? ».
ok("def setting_relevant" in assistant_source and "def mentions_setting" in assistant_source
   and "FOLDER_WORDS = {" in assistant_source and 'page = "10" if program else' in assistant_source
   and 'keep.get_style_context().add_class("suggested-action")' in assistant_source,
   "IA locale : aucune action hors sujet, page Programmes Windows pour un .exe, choix par défaut visible (#1033-#1036)")
# #1042 : ReadFile de zéro octet sur un tube attend des données (libuv) : sinon
# `claude.exe auth login` relançait sa lecture en boucle, 300 Mo par seconde.
ok("fn wait_pipe_readable" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
   and "fn zero_byte_read_on_a_pipe_waits_for_data" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(),
   "tube : lecture de zéro octet bloquante comme sous Windows, plus de boucle de Claude Code (#1042)")
# #1038 : demande d'autorisation sans affichage joignable -> refus pour cette fois, rien de mémorisé.
ok("Gtk.init_check(sys.argv)[0]" in (SCRIPTS / "winlux-permission.py").read_text()
   and "#1038" in (SCRIPTS / "test-permission.py").read_text(),
   "autorisations : affichage injoignable jamais enregistré comme un refus (#1038)")
# #1001 : fermer la fenêtre d'activation avant la fin est annoncé et confirmé.
ok("def stop_confirmed" in assistant_source and '"delete-event"' in assistant_source,
   "Assistant IA : fermer l'activation en cours demande confirmation (#1001)")
# #1002 : « Où est mon fichier » va à l'agent fichiers ; pas de page sans rapport.
ok("FILE_WORDS = re.compile" in assistant_source and "def page_relevant" in assistant_source,
   "IA locale : demande de fichier reconnue, aucune page des Paramètres hors sujet (#1002)")
# ── Barre : Vue des tâches recopiée, contrepoids sans attente (#885, #887) ──
ok("launcher-$number" in (SCRIPTS / "winlux-control.sh").read_text()
   and "X-XFCE-Source" in (SCRIPTS / "winlux-control.sh").read_text(),
   "barre centrée : Vue des tâches et Rechercher reconnus à leur lanceur recopié (#885)")
ok("def _genmon_blank" in (SCRIPTS / "winlux-tray.py").read_text()
   and "Vue des tâches (15) reconnus" in (SCRIPTS / "test-second-session-taskbar.py").read_text(),
   "barre : genmon vide hors du bloc attendu, contrepoids du cache sans attente (#887)")
# #914 : infobulle réseau au nom du Wi-Fi, pas de la carte (« wlo1 »).
ok("def wifi_name" in (SCRIPTS / "winlux-tray.py").read_text()
   and (SCRIPTS / "test-tray-wifi-name.py").exists()
   and "test-tray-wifi-name.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "barre : infobulle Wi-Fi au nom du réseau (#914)")
# #915 : icône d'une fenêtre .exe lancée avec un chemin relatif ou Windows.
ok("def window_target" in (SCRIPTS / "winlux-applications.py").read_text()
   and "/proc/{pid}/cwd" in (SCRIPTS / "winlux-applications.py").read_text()
   and "test-window-icon-target.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "fenêtres .exe : chemin relatif résolu par le dossier du processus, icône du lanceur en repli (#915)")
# #17 : services Windows en Unicode et démarrage automatique.
ok('"StartServiceCtrlDispatcherW"' in (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text()
   and "pub fn autostart" in (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text()
   and '"--services","autostart"' in (SCRIPTS / "winlux-session-startup.py").read_text()
   and "service_suite.exe" in (SCRIPTS / "test-mingw.sh").read_text(),
   "services W (CreateServiceW, StartServiceCtrlDispatcherW…) et démarrage automatique en session (#17)")
# #992 : aide (.chm) et site web (.url) d'un programme dans les applications du menu.
ok("def link_shortcuts" in (SCRIPTS / "winlux-applications.py").read_text()
   and "#992" in (SCRIPTS / "test-native-catalogue.py").read_text()
   and "test-native-catalogue.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "menu Démarrer : raccourcis aide et web d'un programme sous Applications (#992)")
# #1045/#1046 : clé Fonts toujours présente, vue dynamique jamais écrite ni doublée.
ok("fn vue_dynamique_jamais_ecrite_ni_doublee" in (ROOT / "runtime/src/registry/tests.rs").read_text()
   and "live::strip" in (ROOT / "runtime/src/registry/mod.rs").read_text(),
   "registre : clé Fonts toujours là, ni doublon ni police fantôme, vue dynamique hors de registry.tsv (#1045, #1046)")
# #183 : pages de code asiatiques (gconv) et registre en vue dynamique.
ok("fn is_lead_byte" in (ROOT / "runtime/src/intl/dbcs.rs").read_text()
   and "live::overlay" in (ROOT / "runtime/src/registry/mod.rs").read_text()
   and "test-dbcs-gconv.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "pages 932/936/949/950, IsDBCSLeadByte, clés TimeZoneInformation, Nls\\CodePage, Keyboard Layout et Fonts (#183)")
# #925 : menu Démarrer, déplacer une application épinglée.
ok("def move_pin" in (SCRIPTS / "winlux-start-menu.py").read_text()
   and "def pin_drag" in (SCRIPTS / "winlux-start-menu.py").read_text()
   and "test-start-pin-move.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "menu Démarrer : épingle déplacée par clic droit (Déplacer) et glisser-déposer, ordre gardé (#925)")

# #712 : boîtes non modales Rechercher et Remplacer (FindText, ReplaceText).
_find = (ROOT / "runtime/src/dlls/comdlg32_find.rs").read_text()
ok("fn find_proc" in _find and "FR_DIALOGTERM" in _find
   and "fn find32" in (ROOT / "runtime/src/wow32/comdlg.rs").read_text()
   and "registered_id" in (ROOT / "runtime/src/dlls/user32.rs").read_text()
   and "find_replace()" in (ROOT / "testexe/src/bin/commdlg_suite.rs").read_text()
   and "find_replace32()" in (ROOT / "testc/wow32/dlg32.c").read_text(),
   "Rechercher/Remplacer : boîte non modale, FINDMSGSTRING au propriétaire, x86 et crochet (#712)")
ok(all(t in _find for t in ("Remplacer &tout", "Replace &All", "&Alle ersetzen", "Reemplazar &todo")),
   "Rechercher/Remplacer : libellés en français, anglais, allemand et espagnol (#712)")

# #19 : objets job (KILL_ON_JOB_CLOSE, TerminateJobObject, enfants rattachés).
_jobs = (ROOT / "runtime/src/dlls/jobs.rs").read_text()
ok("fn job_tue_ses_membres_a_la_fermeture" in _jobs and "fn terminate_job_object_et_liste_des_pid" in _jobs
   and "jobs::on_child_created(" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
   and "jobs::on_exit();" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(),
   "objets job : membres tués à la fermeture ou à la fin du processus, enfants rattachés, comptage (#19)")

# #20 : objets nommés partagés entre processus du même compte (sections, événements, mutex, sémaphores).
_named = (ROOT / "runtime/src/dlls/named_objects.rs").read_text()
ok("fn objets_nommes_partages_entre_processus" in _named and "fn dossier_prive_au_compte" in _named
   and "meta.uid() == uid()" in _named and "winlux-objets-{}" in _named
   and "fn sections_nommees_de_ntdll_et_de_kernel32" in (ROOT / "runtime/src/dlls/ntdll.rs").read_text()
   and "\"OpenSemaphoreW\"" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(),
   "objets nommés visibles entre processus, cloisonnés par compte (dossier 0700), sections ntdll (#20)")

# #24 : FreeLibrary décharge vraiment (démappage, index TLS rendu, dépendances relâchées).
_deps = (ROOT / "testexe/src/bin/dll_deps.rs").read_text()
ok("image neuve après rechargement" in _deps and "la DLL et sa dépendance sont toutes deux déchargées" in _deps
   and "fn unload(" in (ROOT / "runtime/src/loader.rs").read_text()
   and "fn release_module_tls(" in (ROOT / "runtime/src/process.rs").read_text()
   and "dll_deps" in (ROOT / "scripts/test-runtime.sh").read_text(),
   "FreeLibrary : DLL démappée, TLS libéré, dépendances relâchées, rechargement à neuf (#24)")

# #28 : navigation Tab entre les contrôles de toutes les fenêtres du shell PE.
_wm = (ROOT / "testexe/src/shell/wm.rs").read_text()
ok("pub unsafe fn focus_regression_checks()" in _wm and "crate::gfx::focus_key(app, vk, ch)" in _wm
   and "wm::focus_regression_checks();" in (ROOT / "testexe/src/bin/explorer_regression.rs").read_text()
   and "#28" in (ROOT / "scripts/test-explorer-fixes.py").read_text(),
   "Tab entre les contrôles : cadre d'accent, boîtes modales, champs, quinze fenêtres du shell (#28)")

# ── Barre flottante en deux capsules, maquette 5-B (#889) ──────────────────
ok('status_ids="40 42 ${tray:+10 }' in (SCRIPTS / "winlux-control.sh").read_text()
   and "/panels/panel-2/plugin-ids" in (SCRIPTS / "winlux-control.sh").read_text()
   and "FLOAT_MARGIN" in (SCRIPTS / "winlux-personalize.py").read_text()
   and "deux capsules" in (SCRIPTS / "test-second-session-taskbar.py").read_text(),
   "barre flottante : capsule centrale et capsule d'état en bas à droite, marge transparente, retour au classique sans perte (#889)")

# ── Bascule gauche ↔ centre sans relancer la barre (#920) ──────────────────
ok("genmon-13:refresh:bool:true" in (SCRIPTS / "winlux-control.sh").read_text()
   and "def taskbar_alignment" in (SCRIPTS / "winlux-tray.py").read_text()
   and "barre relancée (#920)" in (SCRIPTS / "test-taskbar-live-align.py").read_text(),
   "barre : même ordre à gauche et au centre, contrepoids rafraîchi en direct, pas de relance (#920)")
# #923 : Paramètres > Navigateurs, éditeur, version et source avant l'installation.
ok("def browser_confirmation" in (SCRIPTS / "winlux-settings.py").read_text()
   and '"--info"' in (SCRIPTS / "prepare-browser-packages.py").read_text()
   and "test-browser-confirm.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "navigateurs : éditeur, version, source et empreinte montrés avant l'installation, confirmer ou refuser (#923)")

# #1056 : logiciels publiés pour Linux par leur éditeur (Claude, ChatGPT…) dans le Store.
ok("class Editeur(Source)" in (SCRIPTS / "winlux-store.py").read_text()
   and "set_default_response(Gtk.ResponseType.CANCEL)" in (SCRIPTS / "winlux-store-confirm.py").read_text()
   and '"winlux-store-repos"' in (SCRIPTS / "winlux-root.py").read_text()
   and "test-store-editeurs.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "Store : logiciels des éditeurs, confirmation (Annuler par défaut), SHA-256, purge et dépôt retiré (#1056)")

# ── Moteur .NET Framework maison, étape 1 (#926) : plusieurs assemblies ──
clr = ROOT / "runtime/src/clr"
ok((clr / "domain.rs").exists() and "fn probe" in (clr / "domain.rs").read_text()
   and "fn resolve_type_ref_in" in (clr / "typesys.rs").read_text()
   and "fn ensure_initialized" in (clr / "il.rs").read_text()
   and "une_bibliotheque_livree_avec_le_programme_est_chargee" in (clr / "tests.rs").read_text()
   and "execute_assembly_in" in (ROOT / "runtime/src/main.rs").read_text(),
   "CLR : DLL .NET privées du dossier de l'appli chargées, appels, héritage, champs et .cctor entre assemblies (#926)")

# ── Moteur .NET Framework, étape 2 (#945, #946) : cœur d'exécution, génériques ──
ok("fn ensure_layout" in (clr / "typesys.rs").read_text()
   and "fn instantiate" in (clr / "typesys.rs").read_text()
   and "O::Constrained" in (clr / "il.rs").read_text()
   and "les_generiques_sont_instancies" in (clr / "tests.rs").read_text()
   and "ref_out_et_acces_indirects" in (clr / "tests.rs").read_text()
   and "les_structures_se_copient" in (clr / "tests.rs").read_text(),
   "CLR : ref/out, structures copiées, génériques instanciés (box/unbox de T, statiques, constrained., ldelem T) (#945, #946)")

# ── Moteur .NET Framework, étape 3 (#971) : délégués, événements, lambdas ──
ok((clr / "delegates.rs").exists()
   and "fn invoke_delegate" in (clr / "delegates.rs").read_text()
   and "O::Ldvirtftn" in (clr / "il.rs").read_text()
   and "les_delegues_appellent_leur_methode" in (clr / "tests.rs").read_text()
   and "ldvirtftn_func_et_evenements" in (clr / "tests.rs").read_text(),
   "CLR : ldftn/ldvirtftn, Invoke, délégués multicast, Combine/Remove, Interlocked des événements (#971)")

# ── Moteur .NET Framework, étape 4 (#972) : filtres, finally imbriqués, rethrow ──
ok("fn run_filter" in (clr / "il.rs").read_text()
   and "O::Endfilter" in (clr / "il.rs").read_text()
   and "INNER_SLOT" in (clr / "exceptions.rs").read_text()
   and "filtres_finally_et_rethrow" in (clr / "tests.rs").read_text()
   and "le_finally_tourne_avant_l_appelant_et_un_filtre_qui_leve_refuse" in (clr / "tests.rs").read_text(),
   "CLR : filtres when, finally imbriqués traversés par leave et par une exception, rethrow, InnerException (#972)")

# ── Moteur .NET Framework, étape 6 (#994) : P/Invoke complet ──
ok("fn call_win64" in (clr / "pinvoke.rs").read_text()
   and "fn callback_for" in (clr / "callbacks.rs").read_text()
   and "fn call_pinvoke" in (clr / "interop.rs").read_text()
   and "fn field_offset_of" in (clr / "metadata.rs").read_text()
   and "p_invoke_rappel_vers_un_delegue" in (clr / "tests.rs").read_text()
   and "p_invoke_structures_et_derniere_erreur" in (clr / "tests.rs").read_text(),
   "CLR : P/Invoke Win64 (flottants, pile, structures C par valeur, rappels natifs, SetLastError) (#994)")

# ── Moteur .NET Framework, étape 5 (#1066) : fils managés ──
ok("fn gil_acquire" in (clr / "threads.rs").read_text()
   and "fn monitor_wait" in (clr / "threads.rs").read_text()
   and "threads::gil_yield()" in (clr / "il.rs").read_text()
   and "threads::wait_foreground()" in (clr / "mod.rs").read_text()
   and "quatre_fils_sous_verrou" in (clr / "tests.rs").read_text()
   and "threadpool_wait_pulse_et_begin_invoke" in (clr / "tests.rs").read_text(),
   "CLR : Thread, lock/Monitor (Wait, Pulse), ThreadPool, Sleep, BeginInvoke/EndInvoke, verrou global aux points sûrs (#1066)")

# ── Icônes cachées une par une derrière la flèche (#927) ───────────────────
ok("GLib.set_prgname(program_name(executable))" in (SCRIPTS / "winlux-notify-icon.py").read_text()
   and 'argv[1] == "overflow"' in (SCRIPTS / "winlux-tray.py").read_text()
   and "def tray_icon_settings" in (SCRIPTS / "winlux-settings-pages.py").read_text()
   and "test-tray-overflow.py" in (ROOT / ".github/workflows/ci.yml").read_text(),
   "zone de notification : chaque programme .exe sous son nom, caché ou montré par icône dans Paramètres (#927)")

# #561 : Paint.NET 5.1 (Direct3D 11.1, Direct2D 1.1 et ses effets personnalisés).
ok((SCRIPTS / "test-paintnet-d2d.py").exists()
   and "test-paintnet-d2d.py" in (ROOT / ".github/workflows/ci.yml").read_text()
   and "uianimation::create_builtin" in (ROOT / "runtime/src/dlls/ole32.rs").read_text(),
   "Paint.NET : Direct2D 1.1, effets personnalisés et gestionnaire d'animations (#561)")
# #984 : DirectWrite 3 à 7 (IDWriteFactory7 de Paint.NET 5.1), jeux de polices.
ok((ROOT / "runtime/src/dlls/dwrite_sets.rs").exists()
   and "VTable<57>" in (ROOT / "runtime/src/dlls/dwrite.rs").read_text()
   and "IDWriteFactory7 : 57 méthodes" in (SCRIPTS / "test-paintnet-d2d.py").read_text(),
   "DirectWrite : IDWriteFactory7, jeux de polices et faces 1 à 6 (#984)")
# #1075 : polices livrées par l'appli (FontFamily par URI dans WPF).
ok("custom::custom_collection as *const ()" in (ROOT / "runtime/src/dlls/dwrite.rs").read_text()
   and "fn read_stream(" in (ROOT / "runtime/src/dlls/dwrite_custom.rs").read_text(),
   "DirectWrite : CreateCustomFontCollection et CreateCustomFontFileReference lisent les polices de l'appli (#1075)")
# #269 : applis .NET autonomes et WinForms. Le ramasse-miettes de coreclr
# suspend ses threads et lit leur contexte ; WaitMessage dort au lieu de
# tourner à 100 % ; une exception non gérée affiche le message .NET, pas
# « Stack overflow. » (page de garde de la pile).
ok((runtime_dlls / "thread_ctl.rs").exists()
   and '"SuspendThread" => fn crate::dlls::kernel32::suspend_thread' in (runtime_dlls / "kernel32_node.rs").read_text()
   and '"SetThreadContext" => fn crate::dlls::kernel32::set_thread_context' in (runtime_dlls / "games_win32.rs").read_text()
   and "fn stack_region(" in (runtime_dlls / "kernel32.rs").read_text()
   and (runtime_dlls / "write_watch.rs").exists()
   and '"GetWriteWatch"=>fn crate::dlls::write_watch::get_write_watch' in (runtime_dlls / "kernel32.rs").read_text()
   and "fn wait_message() -> i32 {\n    loop {" in (runtime_dlls / "user32.rs").read_text()
   and "fn peek_due_timer(" in (runtime_dlls / "gui.rs").read_text()
   and "raise_exception_export()" in (runtime_dlls / "seh.rs").read_text()
   and '"CoRegisterMessageFilter"' in (runtime_dlls / "ole32.rs").read_text()
   and "test-thread-control.sh" in (SCRIPTS / "test-mingw.sh").read_text()
   and "test-thread-control.sh" in (SCRIPTS / "test-all.sh").read_text(),
   ".NET autonome et WinForms : suspension de threads, contexte, WaitMessage, page de garde de pile")

# #1048 : GetNetworkParams, GetIfTable et GetAdaptersInfo rendent une vraie
# taille quand le tampon est trop petit (.NET bouclait sur une taille 0).
iphlp = (runtime_dlls / "iphlpapi.rs").read_text()
ok("const FIXED_INFO_SIZE: usize = 600;" in iphlp and "const IFROW_SIZE: usize = 860;" in iphlp
   and "fn parametres_reseau_taille_puis_contenu" in iphlp
   and "net_params()" in (ROOT / "testc/msvc/threadctl_msvc.c").read_text(),
   "iphlpapi : paramètres réseau et table des interfaces sans boucle sans fin (#1048)")

# #1072 : le pool de threads de .NET 9 appelle GetThreadIOPendingFlag.
ok('"GetThreadIOPendingFlag"=>fn get_thread_io_pending_flag' in (runtime_dlls / "kernel32.rs").read_text()
   and "GetThreadIOPendingFlag" in (ROOT / "docs/compat/kernel32-surface.txt").read_text()
   and "io_pending()" in (ROOT / "testc/msvc/threadctl_msvc.c").read_text(),
   "kernel32 : GetThreadIOPendingFlag pour le pool de threads de .NET 9 (#1072)")

# #1003 : la dictée et les sous-titres ne passent plus par GLib.unix_signal_add (déprécié).
speech = (SCRIPTS / "winlux-speech.py").read_text()
ok("GLibUnix.signal_add" in speech and speech.count("GLib.unix_signal_add") == 1
   and '"Deprecat" not in out.stderr' in (SCRIPTS / "test-speech.py").read_text(),
   "voix : arrêt sur SIGTERM sans avertissement de dépréciation de PyGObject")

# Paramètres : aucun menu déroulant vide (#998), « Adaptation à l'écran » sur la
# politique effective quand display_policy= est vide.
ok("def display_policy" in settings_source and "display_policy(screens)" in settings_source
   and "Jamais de menu vide (#998)" in settings_source
   and "menu vide en" in (SCRIPTS / "test-settings-redesign.py").read_text(),
   "Paramètres : menus Écran et Alimentation jamais vides (#998)")
# ── Débordement de pile (#15) : rattrapé sur le thread principal, et sinon
# rapport EXCEPTION_STACK_OVERFLOW (0xC00000FD) au lieu d'une faute dans Winlux.
ok("fn resume_on_exhausted_stack" in (ROOT / "runtime/src/dlls/seh.rs").read_text()
   and "resume_on_exhausted_stack" in (ROOT / "runtime/src/dlls/seh_hw.rs").read_text()
   and "reported == 0xC000_00FD" in (ROOT / "runtime/src/quality.rs").read_text()
   and "t_main_overflow" in (ROOT / "testc/msvc/hwexc_msvc.c").read_text()
   and "overflow-thread" in (SCRIPTS / "test-hw-exceptions.sh").read_text(),
   "débordement de pile : rattrapé sur tout thread, sinon rapport EXCEPTION_STACK_OVERFLOW (#15)")
# ── Annulation des E/S et handles ou tampons invalides (#18) ──
ok("CancelIoEx(server, &ov) == 1" in (ROOT / "testc/msvc/io_msvc.c").read_text()
   and "test-io-cancel.sh" in (SCRIPTS / "test-mingw.sh").read_text()
   and "test-io-cancel.sh" in (SCRIPTS / "test-all.sh").read_text()
   and "Some(libc::EFAULT) => return 998" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(),
   "E/S : lecture commencée annulée, handles invalides et tampons invalides (ERROR_NOACCESS) (#18)")

# Mise à jour : GitHub en 5xx annoncé comme tel et en 30 s au plus (#1007), liste de
# pause remise à zéro après la reprise (#1008), vrai pluriel des semaines (#1009).
update_apt = (SCRIPTS / "winlux-update-apt.py").read_text(encoding="utf-8")
update_page = (SCRIPTS / "winlux-update-page.py").read_text(encoding="utf-8")
update_test = (SCRIPTS / "test-update-page.py").read_text(encoding="utf-8")
ok("def server_error" in update_apt and '"--max-time", "30"' in update_apt and '"--retry"' in update_apt
   and "recherche en 503" in update_test, "Mise à jour : GitHub en panne n'est pas « aucune version publiée » (#1007)")
ok('self.pause_combo.set_active_id("")' in update_page and "(#1008)" in update_test,
   "Mise à jour : liste de pause revenue à « Choisir une durée » après la reprise (#1008)")
ok("semaine(s)" not in update_page and "def weeks_text" in update_page and "(#1009)" in update_test,
   "Mise à jour : « 1 semaine », « 2 semaines » dans les quatre langues (#1009)")
# Paramètres : un nom de page a la même traduction en barre latérale, en titre,
# dans la recherche du menu Démarrer et chez l'assistant (#1039).
translation_test = (SCRIPTS / "test-translation-keys.py").read_text(encoding="utf-8")
ok("def inconsistent_page_names" in translation_test and "#1039" in translation_test
   and "'Partage et transfert': ('Sharing and transfer', 'Compartir y transferir'" in (SCRIPTS / "winlux-settings-pages.py").read_text(encoding="utf-8"),
   "Paramètres : noms de page traduits pareil partout, quatre langues (#1039)")
# Shell PE, fiche 28 : Échap redessine (#1023), flèches dans le menu du
# Gestionnaire des tâches (#1024), titre traduit (#1025), WINLUX_LANG lu
# (#1026), Bloc-notes ouvrable seul (#1027). Vérifiés dans le vrai runtime
# par explorer_regression (test-runtime.sh).
pe_wm = (ROOT / "testexe/src/shell/wm.rs").read_text(encoding="utf-8")
pe_lib = (ROOT / "testexe/src/lib.rs").read_text(encoding="utf-8")
ok("frame_shown && !crate::gfx::focus_state(app).1" in pe_wm and "(#1023)" in pe_wm,
   "Shell PE : Échap efface le cadre de focus et redessine (#1023)")
ok("focus_step(FOCUS_APP" in (ROOT / "testexe/src/shell/taskmgr.rs").read_text(encoding="utf-8") and "(#1024)" in pe_wm,
   "Shell PE : Haut/Bas dans le menu contextuel du Gestionnaire des tâches (#1024)")
ok("wm::title_of(app)" in (ROOT / "testexe/src/bin/gui_demo.rs").read_text(encoding="utf-8") and "(#1025)" in pe_wm,
   "Shell PE : titre de fenêtre dans la langue de l'interface (#1025)")
ok("pub fn winlux_lang_env" in pe_lib and "winlux_lang_env()" in (ROOT / "testexe/src/shell/locale.rs").read_text(encoding="utf-8")
   and "language_regression_checks" in (ROOT / "testexe/src/bin/explorer_regression.rs").read_text(encoding="utf-8"),
   "Shell PE : WINLUX_LANG choisit la langue (#1026)")
ok("export WINLUX_SHELL_APP=notepad" in (SCRIPTS / "winlux-shell-app.sh").read_text(encoding="utf-8"),
   "Shell PE : winlux-shell-app notepad (#1027)")
# #16 : activation COM depuis le registre (InprocServer32 → DllGetClassObject),
# CoCreateInstanceEx, CoFreeUnusedLibraries ; vérifiée par com_suite.exe.
com_registry = (ROOT / "runtime/src/dlls/com_registry.rs").read_text(encoding="utf-8")
ok("DllGetClassObject" in com_registry and "fn free_unused" in com_registry
   and "co_create_instance_ex" in (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8")
   and "com_suite.exe" in (SCRIPTS / "test-mingw.sh").read_text(encoding="utf-8"),
   "COM : activation par InprocServer32, CoCreateInstanceEx, CoFreeUnusedLibraries (#16)")
# Transfert de réglages (fiche #171 point 7) : sous le compte (#1100),
# apparence de la session native (#1101), état dans son dossier (#1102).
transfer_script = (SCRIPTS / "winlux-transfer.sh").read_text(encoding="utf-8")
ok("        transfer-*) return 0 ;;" in (SCRIPTS / "winlux-control.sh").read_text(encoding="utf-8")
   and "appearance_export" in transfer_script and "WINLUX_TRANSFER_RUN" in transfer_script
   and "#1101" in (SCRIPTS / "test-transfer.sh").read_text(encoding="utf-8"),
   "Transfert de réglages : sous le compte, avec thème, accent et fond de la session native (#1100, #1101, #1102)")
print(f"Campagne v0.2.13 : {checks} régressions vérifiées.")
