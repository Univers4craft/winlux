#!/usr/bin/python3
"""Centre de notifications (#3058) : toute notification gardée par le démon
(200 au plus) se relit dans le centre. 50 cartes à l'ouverture, les suivantes
au bas du défilement, jusqu'à la plus ancienne.

    xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session -- python3 -B scripts/test-notification-historique.py
"""
import json
import os
from pathlib import Path
import runpy
import sys
import tempfile

HERE = Path(__file__).resolve().parent
home = Path(tempfile.mkdtemp(prefix="notifications-historique-"))
os.environ.update(HOME=str(home), XDG_STATE_HOME=str(home / "state"), XDG_CONFIG_HOME=str(home / "config"),
                  XDG_CACHE_HOME=str(home / "cache"), WINLUX_LANG="fr", NO_AT_BRIDGE="1")
(home / "state/winlux").mkdir(parents=True)
# Même borne que le démon : historique plein, du n° 1 (le plus ancien) au n° 200.
kept = [dict(id=n, app="Essai", title=f"Notification n° {n}", body="", actions=[], time=1793520000 + n) for n in range(1, 201)]
(home / "state/winlux/notifications.json").write_text(json.dumps(kept))
module = runpy.run_path(str(HERE / "winlux-popup.py"), run_name="test_3058")
GLib, Gtk = module["GLib"], module["Gtk"]
app = module["Popup"]("notifications")
seen, problems = {}, []


def titles():
    found = []
    def walk(widget):
        if isinstance(widget, Gtk.Label) and widget.get_text().startswith("Notification n° "):
            found.append(int(widget.get_text().split()[-1]))
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): walk(child)
    walk(app.cards)
    return found


def bottom():
    adjustment = app.scroll.get_vadjustment()
    adjustment.set_value(adjustment.get_upper() - adjustment.get_page_size())


def step():
    try:
        shown = titles()
        rounds = seen.setdefault("rounds", [])
        rounds.append(len(shown))
        print(f"cartes affichées : {len(shown)} (plus ancienne : n° {min(shown) if shown else '-'})", flush=True)
        if len(shown) < 200 and len(rounds) < 8:
            bottom(); GLib.timeout_add(250, step); return False
        seen["final"] = shown
    except Exception as error:
        problems.append(error)
    app.quit()
    return False


GLib.timeout_add(800, step)
app.run([])
if problems: raise problems[0]
rounds, final = seen.get("rounds", []), seen.get("final", [])
ok = rounds[:1] == [50] and final == list(range(200, 0, -1))
print(("OK   " if ok else "ÉCHEC ") + "200 gardées : 50 à l'ouverture, toutes relisibles en défilant, plus récentes d'abord (#3058)")
sys.exit(0 if ok else 1)
