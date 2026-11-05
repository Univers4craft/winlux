#!/usr/bin/python3
"""Paramètres › Comptes › Sauvegarde (page 38, native seulement).

La page lit l'état écrit par winlux-backup (relais root sur le système
installé) : destination choisie parmi les disques réellement branchés,
planification, dossiers, historique et restauration « conserver les deux »
par défaut. Sans disque, un message remplace la liste (jamais grisée).
"""
import os
from pathlib import Path
import runpy
import sys
import tempfile

HERE = Path(__file__).resolve().parent
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


tmp = tempfile.TemporaryDirectory()
run_dir = Path(tmp.name) / "run"
state = run_dir / "backup" / "alice"
state.mkdir(parents=True)
os.environ["WINLUX_RUN_DIR"] = str(run_dir)
os.environ["WINLUX_BACKUP_USER"] = "alice"
os.environ.setdefault("WINLUX_LANG", "fr")

settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="backup_page_test")
Gtk, GLib = settings["Gtk"], settings["GLib"]
pages = settings["EXTRA"]
PAGE = pages.PAGE_BACKUP

# Numéro et place : 38, rangée sous Comptes comme « Sauvegarde Windows ».
check(PAGE == 38 and settings["PAGES"][PAGE] == "Sauvegarde", "page 38 « Sauvegarde »")
check(any(PAGE in members for name, members in settings["CATEGORIES"] if name == "Comptes"), "Sauvegarde sous Comptes")
check(pages.page_available(PAGE, {}), "page visible quand l'état de sauvegarde existe")
check(settings["page_argument"]("ms-settings:backup") == PAGE and settings["page_argument"]("sauvegarde") == PAGE,
      "ms-settings:backup et « winlux-settings sauvegarde » ouvrent la page")
check(pages.backup_snapshot_date("20261105-103118") == "05/11/2026 10:31", "date d'une sauvegarde")
share = pages.BACKUP_SHARE
check(share.match("smb://nas/sauvegardes") and share.match("nas:/srv/backup") and share.match("nfs://nas/srv"), "adresses de partage valides")
check(not share.match("/etc") and not share.match("smb://nas") and not share.match("smb://nas/a\nb"), "adresses de partage refusées")

HOME = "/home/alice"


def write_state(destination="", drives=(), snapshots=(), **extra):
    lines = [f"enabled={extra.get('enabled', '0')}", f"destination={destination}", "frequency=60", "keep_days=0",
             f"last_result={extra.get('last_result', '')}", f"running={extra.get('running', '0')}",
             f"network={extra.get('network', '0')}", f"available={extra.get('available', '0')}", f"home={HOME}",
             f"folder=1|{HOME}/Documents", f"folder=0|{HOME}/Bureau", "folders=2"]
    if extra.get("last_run_text"): lines.append(f"last_run_text={extra['last_run_text']}")
    if extra.get("same_disk"): lines.append("same_disk=1")
    (state / "status.txt").write_text("\n".join(lines) + "\n")
    (run_dir / "backup" / "drives.txt").write_text("".join(f"drive={d}|12.0 Go\n" for d in drives))
    (state / "snapshots.txt").write_text("".join(f"{s}|x|{n}\n" for s, n in snapshots))


def walk(widget):
    yield widget
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children(): yield from walk(child)


window = settings["Settings"]()
calls = []
window.execute = lambda action, *args: calls.append((action, *args))


def show():
    window.open_page(PAGE)
    widgets = list(walk(window.body))
    labels = [w.get_text() for w in widgets if isinstance(w, Gtk.Label)]
    buttons = {w.get_label(): w for w in widgets if isinstance(w, Gtk.Button) and w.get_label()}
    combos = [w for w in widgets if isinstance(w, Gtk.ComboBoxText)]
    switches = [w for w in widgets if isinstance(w, Gtk.Switch)]
    return labels, buttons, combos, switches


def scenario():
    try:
        # 1. Rien de branché, rien de choisi : message clair, pas de liste vide.
        write_state()
        labels, buttons, combos, switches = show()
        check(window.current == PAGE, "la page s'ouvre")
        check(any("Aucun disque externe détecté" in t for t in labels), "message sans disque")
        check(not combos and not switches, "ni liste de disques ni planification sans destination")
        check(not buttons["Sauvegarder maintenant"].get_sensitive(), "« Sauvegarder maintenant » sans destination")
        check("Partage réseau…" in buttons and "Ajouter un dossier…" in buttons, "partage réseau et ajout de dossier proposés")
        check(any(t == "Documents" for t in labels), "dossier affiché par son nom")
        check(any("(dossier absent, ignoré)" in t for t in labels), "dossier absent signalé")
        check("Historique et restauration" not in labels, "pas d'historique sans destination")

        # 2. Une clé branchée : la choisir enregistre la destination.
        write_state(drives=["/media/alice/CLE"])
        labels, buttons, combos, switches = show()
        check(len(combos) == 1, "liste des disques branchés")
        combos[0].set_active_id("/media/alice/CLE")
        check(("backup-set", "alice", "DESTINATION", "/media/alice/CLE") in calls, f"destination enregistrée : {calls}")
        calls.clear()

        # 3. Destination choisie et branchée, deux sauvegardes.
        write_state("/media/alice/CLE", ["/media/alice/CLE"], [("20261104-090000", 12), ("20261105-103118", 14)],
                    enabled="1", available="1", last_result="ok", last_run_text="05/11/2026 10:31", same_disk="1")
        labels, buttons, combos, switches = show()
        check(buttons["Sauvegarder maintenant"].get_sensitive(), "« Sauvegarder maintenant » actif")
        check(switches and switches[0].get_active(), "sauvegarde automatique active")
        check(len(combos) == 3, f"disque, fréquence, conservation : {len(combos)}")
        check(any("même disque" in t for t in labels), "avertissement même disque")
        check("Historique et restauration" in labels, "historique affiché")
        history = labels[labels.index("Historique et restauration"):]
        dates = [t for t in history if t.startswith(("05/11/2026", "04/11/2026"))]
        check(dates[:2] == ["05/11/2026 10:31", "04/11/2026 09:00"], f"plus récente en premier : {dates}")
        buttons["Sauvegarder maintenant"].clicked()
        check(("backup-run", "alice") in calls, "sauvegarde lancée")
        switches[0].set_active(False)
        check(("backup-set", "alice", "ENABLED", "0") in calls, "planification coupée")
        combos[1].set_active_id("1440")
        check(("backup-set", "alice", "FREQUENCY", "1440") in calls, "fréquence quotidienne")

        # 4. Disque débranché : il reste nommé, sans historique.
        write_state("/media/alice/CLE", [], enabled="1", available="0", last_result="unavailable")
        labels, buttons, combos, switches = show()
        check(any("Destination indisponible" in t for t in labels), "destination indisponible signalée")
        check(any("(non branché)" in t for t in [combos[0].get_active_text() or ""]), "disque non branché nommé")
        check("Historique et restauration" not in labels, "pas d'historique disque débranché")
    except Exception as error:  # noqa: BLE001
        failures.append(repr(error))
        raise
    finally:
        window.destroy()
        Gtk.main_quit()
    return False


GLib.idle_add(scenario)
Gtk.main()

# Quatre langues : chaque texte de la page est traduit.
import re
source = (HERE / "winlux-settings-pages.py").read_text()
segment = source[source.index("# ── Sauvegarde : mes dossiers"):source.index("    def page_sharing(self):")]
texts = set(re.findall(r'_t\("((?:[^"\\]|\\.)*)"\)', segment)) | set(re.findall(r'\(\d+, "([^"]+)"\)', segment))
table = pages._TR
missing = sorted(t for t in texts if t not in table or len(table[t]) != 3 or not all(table[t]))
check(not missing, f"textes non traduits : {missing}")

# Relais root : réglages et restauration permis au compte pour lui-même.
root_tool = runpy.run_path(str(HERE / "winlux-root.py"), run_name="backup_root_test")
for action in ("backup-set", "backup-add", "backup-remove", "backup-share", "backup-restore", "backup-run"):
    check(root_tool["control_level"]([action, "alice", "x"]) == "user", f"relais : {action}")
control = (HERE / "winlux-control.sh").read_text()
check("backup-set|backup-add|backup-remove|backup-share" in control and "backup-restore)" in control, "winlux-control : actions de la page")

if failures:
    sys.exit(1)
print("Paramètres › Sauvegarde : OK")
