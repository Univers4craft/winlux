#!/usr/bin/python3
"""Centre de notifications (#3342) : une notification qui arrive pendant la
lecture ne reconstruit pas la liste. Les cartes chargées restent, la même
carte reste en vue, et le seul marqueur « nouvelle » (MarkSeen) ne refait
aucune carte.

    xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session -- python3 -B scripts/test-notification-lecture.py
"""
import json
import os
from pathlib import Path
import runpy
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
home = Path(tempfile.mkdtemp(prefix="notifications-lecture-"))
os.environ.update(HOME=str(home), XDG_STATE_HOME=str(home / "state"), XDG_CONFIG_HOME=str(home / "config"),
                  XDG_CACHE_HOME=str(home / "cache"), WINLUX_LANG="fr", NO_AT_BRIDGE="1")
(home / "state/winlux").mkdir(parents=True)
state = home / "state/winlux/notifications.json"
kept = [dict(id=n, app="Essai", title=f"Notification n° {n}", body="", actions=[], time=1793520000 + n, nouvelle=True)
        for n in range(1, 201)]
state.write_text(json.dumps(kept))
module = runpy.run_path(str(HERE / "winlux-popup.py"), run_name="test_3342")
GLib, Gtk = module["GLib"], module["Gtk"]
app = module["Popup"]("notifications")
results, problems = [], []


def check(cond, label):
    results.append(cond)
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)


def pump():
    for _ in range(5):
        while Gtk.events_pending():
            Gtk.main_iteration()
        time.sleep(0.02)


def number(card):
    found = []
    def walk(widget):
        if isinstance(widget, Gtk.Label) and widget.get_text().startswith("Notification n° "):
            found.append(int(widget.get_text().split()[-1]))
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children():
                walk(child)
    walk(card)
    return found[0] if found else None


def top_card():
    value = app.scroll.get_vadjustment().get_value()
    for card in app.cards.get_children():
        area = card.get_allocation()
        if area.y + area.height > value:
            return card
    return None


def write(history):
    state.write_text(json.dumps(history))
    app.refresh_notifications()
    pump()


def scenario():
    try:
        adjustment = app.scroll.get_vadjustment()
        while app.pending:                       # lecture jusqu'aux plus anciennes
            adjustment.set_value(adjustment.get_upper() - adjustment.get_page_size()); pump()
        cards = app.cards.get_children()
        adjustment.set_value(cards[120].get_allocation().y + 5); pump()
        reading, value = top_card(), adjustment.get_value()
        before = {id(card) for card in cards}
        print(f"en lecture : n° {number(reading)}, {len(cards)} cartes, défilement {value:.0f} px")

        # Le centre ouvert marque tout vu : le démon réécrit l'historique.
        for item in kept:
            item.pop("nouvelle", None)
        write(kept)
        check({id(card) for card in app.cards.get_children()} == before and top_card() is reading,
              "MarkSeen : aucune carte refaite, même carte en vue")

        # Une notification arrive au-dessus de la lecture.
        kept.append(dict(id=201, app="Essai", title="Notification n° 201", body="", actions=[], time=1793520201, nouvelle=True))
        write(kept)
        cards = app.cards.get_children()
        print(f"après arrivée : n° {number(top_card())}, {len(cards)} cartes, défilement {adjustment.get_value():.0f} px")
        check(len(cards) == 201 and number(cards[0]) == 201, "nouvelle carte en tête, les 200 chargées restent")
        check(top_card() is reading, "la même carte reste en vue")
        check(len({id(card) for card in cards} & before) == 200, "aucune carte existante refaite")

        # Une notification effacée ailleurs : seule sa carte part.
        write([item for item in kept if item["id"] != 150])
        numbers = [number(card) for card in app.cards.get_children()]
        check(150 not in numbers and len(numbers) == 200 and top_card() is reading, "effacement : une seule carte retirée")

        # Tout effacé : le message d'historique vide revient.
        write([])
        check([child.get_text() for child in app.cards.get_children() if isinstance(child, Gtk.Label)] == ["Aucune nouvelle notification"],
              "historique vide : message affiché")
    except Exception as error:
        problems.append(error)
    app.quit()
    return False


GLib.timeout_add(800, scenario)
app.run([])
if problems:
    raise problems[0]
ok = results and all(results)
print(("OK   " if ok else "ÉCHEC ") + "Lecture du centre gardée quand une notification arrive (#3342)")
sys.exit(0 if ok else 1)
