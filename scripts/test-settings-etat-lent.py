#!/usr/bin/python3
"""Paramètres : une commande d'état lente ne retarde plus la page (#3021).

Imprimantes et scanners (26) attendait `lpstat` deux fois, Partage et
transfert (28) `winlux-control share-status` puis `transfer-status` (chacun
suivi de toute la tournée `status`) avant que la fenêtre ne s'affiche :
1,8 s et 2,7 s sur le PC du banc contre ~0,7 s pour les autres pages.

Ici les commandes sont des faux qui dorment 1 s :
1. la page s'affiche en moins de 0,5 s, état « en cours » ou dernier connu ;
2. elle se complète seule quand la commande a répondu (une seule fois) ;
3. winlux-control est appelé sans la tournée status (WINLUX_CONTROL_NO_STATUS=1) ;
4. une réponse arrivée après qu'on a changé de page ne touche pas la page affichée.
"""
import os
from pathlib import Path
import runpy
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
failures = []


def check(condition, message):
    print(("OK   " if condition else "ÉCHEC ") + message)
    if not condition: failures.append(message)


tmp = Path(tempfile.mkdtemp())
run_dir, bin_dir, log = tmp / "run", tmp / "bin", tmp / "appels"
run_dir.mkdir(); bin_dir.mkdir()
fakes = {
    "lpstat": f"""echo "lpstat $*" >> {log}
sleep 1
case "$1" in -d) echo "destination système par défaut : Bureau" ;; -e) printf 'Bureau\\nAtelier\\n' ;; esac""",
    "winlux-control": f"""echo "winlux-control $* NO_STATUS=${{WINLUX_CONTROL_NO_STATUS:-}}" >> {log}
sleep 1
case "$1" in
  share-status) printf 'available=1\\nenabled=1\\nhost=pc\\nshare=1|Photos|lecture\\n' > {run_dir}/shares.txt ;;
  transfer-status) printf 'target=0|Clé USB TEST\\n' > {run_dir}/transfer.txt ;;
esac""",
    "winlux-share": "exit 0", "winlux-transfer": "exit 0",
}
for name, body in fakes.items():
    (bin_dir / name).write_text("#!/bin/sh\n" + body + "\n")
    (bin_dir / name).chmod(0o755)
os.environ["PATH"] = f"{bin_dir}:{os.environ['PATH']}"
os.environ["WINLUX_RUN_DIR"] = str(run_dir)
os.environ["WINLUX_LANG"] = "fr"

settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="etat_lent_test")
Gtk, GLib = settings["Gtk"], settings["GLib"]
pages = settings["EXTRA"]


def walk(widget):
    yield widget
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children(): yield from walk(child)


def labels():
    return [w.get_text() for w in walk(window.body) if isinstance(w, Gtk.Label)]


def calls():
    return log.read_text().splitlines() if log.exists() else []


def pump_until(cond, seconds=6):
    end = time.monotonic() + seconds
    while time.monotonic() < end and not cond():
        while Gtk.events_pending(): Gtk.main_iteration_do(False)
        time.sleep(0.01)
    while Gtk.events_pending(): Gtk.main_iteration_do(False)
    return cond()


def timed_open(page):
    start = time.monotonic()
    window.open_page(page)
    return (time.monotonic() - start) * 1000


window = settings["Settings"]()
window.show_all()
check(pages.PAGE_PRINTERS == 26 and pages.PAGE_SHARING == 28, "pages 26 et 28")

# Imprimantes : affichée tout de suite, complétée ensuite, lpstat une fois.
ms = timed_open(26)
print(f"page 26 ouverte en {ms:.0f} ms ; textes : {labels()}")
check(ms < 500, f"Imprimantes s'ouvre sans attendre lpstat ({ms:.0f} ms)")
check("Recherche des imprimantes…" in labels(), "« Recherche des imprimantes… » pendant la lecture")
filled = pump_until(lambda: "Atelier" in labels())
print(f"textes après réponse : {labels()} ; appels : {calls()}")
check(filled, "les imprimantes apparaissent quand CUPS a répondu")
check("Imprimante par défaut" in labels(), "imprimante par défaut reconnue")
pump_until(lambda: False, 1.5)
check(sorted(calls()) == ["lpstat -d", "lpstat -e"], f"lpstat lancé une fois par ouverture : {calls()}")

# Partage : affiché tout de suite, winlux-control sans tournée status.
log.unlink()
ms = timed_open(28)
print(f"page 28 ouverte en {ms:.0f} ms ; textes : {labels()}")
check(ms < 500, f"Partage et transfert s'ouvre sans attendre winlux-control ({ms:.0f} ms)")
filled = pump_until(lambda: "Photos" in labels() and any("Clé USB TEST" in (c.get_active_text() or "")
                    for c in walk(window.body) if isinstance(c, Gtk.ComboBoxText)))
print(f"textes après réponse : {labels()} ; appels : {calls()}")
check(filled, "dossier partagé et emplacement d'export affichés après la réponse")
pump_until(lambda: False, 1.5)
check(sorted(calls()) == ["winlux-control share-status NO_STATUS=1", "winlux-control transfer-status NO_STATUS=1"],
      f"un appel de chaque, sans tournée status : {calls()}")

# Réponse tardive après un changement de page : la page affichée ne bouge pas.
(run_dir / "shares.txt").unlink()
window.open_page(28)
window.open_page(11)
shown = labels()
pump_until(lambda: False, 2.5)
check(window.current == 11 and labels() == shown, "une réponse tardive ne reconstruit pas une autre page")

window.destroy()
if failures:
    sys.exit("ÉCHEC (#3021) : " + " ; ".join(failures))
print("Paramètres : états lents lus hors du fil GTK (#3021)")
