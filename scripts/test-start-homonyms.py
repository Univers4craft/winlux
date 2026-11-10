#!/usr/bin/python3
"""Démarrer : homonymes départagés et distingués (#1489).

Claude pour Windows (winlux-pe-…) et Claude de la Boutique portent le même
nom : le Meilleur résultat est l'application installée le plus récemment, et
le repère « .exe » distingue les deux lignes.

    xvfb-run -a python3 -B scripts/test-start-homonyms.py
"""
import os
import runpy
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
tmp = Path(tempfile.mkdtemp(prefix="winlux-start-homonyms-"))
os.environ.update(HOME=str(tmp), WINLUX_LANG="fr", XDG_DATA_HOME=str(tmp / "share"))
module = runpy.run_path(str(HERE / "winlux-start-menu.py"), run_name="start_homonyms_test")
result_order, homonyms = module["result_order"], module["homonyms"]


class App:
    def __init__(self, ident, name):
        self.ident, self.name = ident, name

    def get_id(self):
        return self.ident

    def get_display_name(self):
        return self.name


exe = App("winlux-pe-claude-1a2b.desktop", "Claude")
store = App("claude-desktop.desktop", "Claude")
other = App("clamtk.desktop", "ClamTk")
apps = [exe, other, store]


def best(query, state):
    return [app.get_id() for app in sorted(apps, key=lambda app: result_order(app, query, state))]


# Boutique installée après le .exe : elle passe en premier, quel que soit l'ordre de Gio.
state = {"seen": {exe.get_id(): 0, store.get_id(): 1_800_000_000, other.get_id(): 0}}
for order in (apps, list(reversed(apps))):
    apps[:] = order
    assert best("claude", state)[0] == store.get_id(), best("claude", state)
# .exe installé en dernier : c'est lui le Meilleur résultat.
state = {"seen": {exe.get_id(): 1_900_000_000, store.get_id(): 1_800_000_000}}
assert best("claude", state)[0] == exe.get_id(), best("claude", state)
# Même date (applications présentes avant la première ouverture du menu) :
# l'application Linux d'abord, puis l'ordre est stable.
state = {"seen": {exe.get_id(): 0, store.get_id(): 0}}
assert best("claude", state)[:2] == [store.get_id(), exe.get_id()], best("claude", state)
assert best("claude", {}) == best("claude", {"seen": "abîmé"})
# La pertinence reste le premier critère.
assert best("clam", state)[0] == other.get_id()
assert homonyms(apps) == {"claude"} and homonyms([exe, other]) == set()
print("Démarrer : homonymes départagés et distingués (#1489) : OK")
