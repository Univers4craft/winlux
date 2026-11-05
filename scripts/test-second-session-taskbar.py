#!/usr/bin/python3
"""Barre des tâches d'un compte ouvert par « Changer d'utilisateur ».

Relevé VM du 5 novembre 2026 : le compte camille, créé après la mise à jour
et ouvert en 2e session, avait le bouton Démarrer à x≈838, ni à gauche
(TASKBAR_ALIGN=left) ni au milieu. Ordre de ses greffons :
3 1 2 8 10 12 4 5 6 9 7 13. C'est la copie figée de xfce4-panel.xml, avec le
séparateur 3 étiré et le contrepoids 13 au bout. winlux-user-session
n'appliquait l'alignement que dans la 1re session.

1. winlux-user-session applique l'alignement dans toute session.
2. Sur l'état de camille (xfconf simulé), align-left et align-center donnent
   le bon ordre et le bon étirement, sans relancer la barre.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


# ── 1. Toute session applique l'alignement ──────────────────────────────────
session = (ROOT / "scripts/winlux-user-session.sh").read_text()
block = session[session.index('if test "$primary" != 1 && test -x /usr/bin/winlux-control; then ('):]
block = block[:block.index("\nfi\n")]
ok('winlux-control "align-$alignment"' in block and "TASKBAR_ALIGN" in block,
   "2e session : l'alignement de la barre est appliqué")
ok(session.index('if test "$primary" != 1 && test -x /usr/bin/winlux-control') >
   session.index("start panel.log /usr/bin/xfce4-panel"),
   "alignement après le lancement de la barre (xfconf la met à jour en direct)")

# ── 2. État de camille corrigé par winlux-control ───────────────────────────
template = (ROOT / "integration/native/config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").read_text()
ok('value="13"/></property>' in template and 'name="plugin-3" type="string" value="separator"><property name="expand" type="bool" value="true"/>' in template,
   "copie figée du modèle : séparateur 3 étiré et contrepoids 13 au bout (la cause)")

FAKE = r'''#!/usr/bin/python3
import json, os, sys
store = os.environ["FAKE_XFCONF"]
data = json.load(open(store))
args = sys.argv[1:]
prop = args[args.index("-p") + 1]
values = [args[i + 1] for i, a in enumerate(args) if a == "-s"]
with open(os.environ["FAKE_XFCONF_LOG"], "a") as log:
    log.write(" ".join(args) + "\n")
if values:
    data[prop] = values if "-a" in args else values[0]
    json.dump(data, open(store, "w"))
    sys.exit(0)
if prop not in data:
    sys.exit(1)
value = data[prop]
if isinstance(value, list):
    print(f"Value is an array with {len(value)} items:\n")
    print("\n".join(value))
else:
    print(value)
'''

with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    (work / "bin").mkdir()
    (work / "root/etc/winlux").mkdir(parents=True)
    (work / "home").mkdir()
    fake = work / "bin/xfconf-query"
    fake.write_text(FAKE)
    fake.chmod(0o755)
    for tool in ("pgrep", "xfce4-panel"):
        (work / "bin" / tool).write_text(f'#!/bin/sh\necho "{tool} $*" >> "$FAKE_XFCONF_LOG"\nexit 1\n')
        (work / "bin" / tool).chmod(0o755)
    store, log = work / "xfconf.json", work / "xfconf.log"

    def camille():
        state = {"/panels/panel-1/plugin-ids": "3 1 2 8 10 12 4 5 6 9 7 13".split(),
                 "/plugins/plugin-3": "separator", "/plugins/plugin-3/expand": "true",
                 "/plugins/plugin-10": "systray", "/plugins/plugin-12": "genmon"}
        for number in (4, 5, 6, 7, 9, 13):
            state[f"/plugins/plugin-{number}"] = "genmon"
        store.write_text(json.dumps(state))
        log.write_text("")

    def align(how):
        result = subprocess.run(
            ["sh", str(ROOT / "scripts/winlux-control.sh"), f"align-{how}"],
            env={**os.environ, "PATH": f"{work}/bin:/usr/bin:/bin", "WINLUX_CONTROL_ROOT": str(work / "root"),
                 "HOME": str(work / "home"), "XDG_CONFIG_HOME": str(work / "home/.config"),
                 "WINLUX_CONTROL_NO_STATUS": "1", "WINLUX_ROOT_SOCKET": str(work / "absent.sock"),
                 "FAKE_XFCONF": str(store), "FAKE_XFCONF_LOG": str(log)},
            capture_output=True, text=True, timeout=30)
        state = json.loads(store.read_text())
        return result, " ".join(state["/panels/panel-1/plugin-ids"]), state["/plugins/plugin-3/expand"]

    camille()
    result, ids, expand = align("left")
    ok(result.returncode == 0, f"align-left réussit ({result.stderr.strip()[:200]})")
    ok(ids == "3 1 2 8 10 12 4 5 6 9 7 13" and expand == "false",
       f"à gauche : séparateur 3 non étiré, Démarrer contre le bord (ids {ids}, expand {expand})")
    ok("xfce4-panel -r" not in log.read_text(), "pas de relance de la barre (13 déjà chargé)")

    camille()
    result, ids, expand = align("center")
    ok(result.returncode == 0, "align-center réussit")
    ok(ids.startswith("3 13 1 2 8") and expand == "true",
       f"centrée : contrepoids 13 juste après le séparateur 3 (ids {ids})")
    ok(re.fullmatch(r"3 13 1 2 8 10 12 4 5 6 9 7", ids) is not None, "centrée : zone de notification et clavier gardés")
    ok("xfce4-panel -r" not in log.read_text(), "centrée : pas de relance de la barre")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("OK: barre d'une 2e session alignée comme la 1re")
