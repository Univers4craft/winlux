#!/usr/bin/python3
"""Barre des tâches d'un compte ouvert par « Changer d'utilisateur ».

Relevé VM du 5 novembre 2026 : le compte camille, créé après la mise à jour
et ouvert en 2e session, avait le bouton Démarrer à x≈838, ni à gauche
(TASKBAR_ALIGN=left) ni au milieu. Ordre de ses greffons :
3 1 2 8 10 12 4 7 9 13 (depuis #703). C'est la copie figée de xfce4-panel.xml, avec le
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
        state = {"/panels/panel-1/plugin-ids": "3 1 2 8 10 12 4 7 9 13".split(),
                 "/plugins/plugin-3": "separator", "/plugins/plugin-3/expand": "true",
                 "/plugins/plugin-10": "systray", "/plugins/plugin-12": "genmon"}
        for number in (4, 7, 9, 13):  # barre de #703 : réseau/son/batterie = 4
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
    ok(ids == "3 1 2 8 10 12 4 7 9 13" and expand == "false",
       f"à gauche : séparateur 3 non étiré, Démarrer contre le bord (ids {ids}, expand {expand})")
    ok("xfce4-panel -r" not in log.read_text(), "pas de relance de la barre (13 déjà chargé)")

    camille()
    result, ids, expand = align("center")
    ok(result.returncode == 0, "align-center réussit")
    ok(ids.startswith("3 13 1 2 8") and expand == "true",
       f"centrée : contrepoids 13 juste après le séparateur 3 (ids {ids})")
    ok(re.fullmatch(r"3 13 1 2 8 10 12 4 7 9", ids) is not None, "centrée : zone de notification et clavier gardés")
    ok("xfce4-panel -r" not in log.read_text(), "centrée : pas de relance de la barre")

# ── 3. Fichiers propres au compte (relevé VM du 7 novembre 2026) ───────────
# /run/winlux est collant : action.txt du 1er compte (0600) ne pouvait pas
# être remplacé par camille, winlux-control s'arrêtait sur « Opération en
# cours… » avant d'agir, et la barre restait avec 13 au bout.
control = (ROOT / "scripts/winlux-control.sh").read_text()
ok('mine="$run/control-$(id -u)"' in control and 'log="$mine/action.log"' in control
   and 'mkdir "$mine/control.lock"' in control, "winlux-control : message, journal et verrou par compte")
ok('control_tmp=$(fresh "$run/control.txt")' in control and 'publish "$control_tmp" "$run/control.txt"' in control,
   "relevé partagé : un compte qui ne peut pas le remplacer n'échoue pas")
with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    for sub in ("bin", "root/etc/winlux", "home"):
        (work / sub).mkdir(parents=True)
    fake = work / "bin/xfconf-query"
    fake.write_text(FAKE)
    fake.chmod(0o755)
    for tool in ("pgrep", "xfce4-panel"):
        (work / "bin" / tool).write_text("#!/bin/sh\nexit 1\n")
        (work / "bin" / tool).chmod(0o755)
    run = work / "root/run/winlux"
    run.mkdir(parents=True)
    store, log = work / "xfconf.json", work / "xfconf.log"
    state = {"/panels/panel-1/plugin-ids": "3 1 15 14 2 8 10 12 4 7 9 13".split(),
             "/plugins/plugin-3": "separator", "/plugins/plugin-3/expand": "true",
             "/plugins/plugin-10": "systray", "/plugins/plugin-12": "genmon",
             "/plugins/plugin-15": "launcher", "/plugins/plugin-15/items": ["winlux-search.desktop"],
             "/plugins/plugin-14": "launcher", "/plugins/plugin-14/items": ["winlux-taskview.desktop"]}
    store.write_text(json.dumps(state))
    log.write_text("")
    # Le compte de test n'est pas root (en root sur ce poste : nobody).
    as_user = []
    if os.getuid() == 0:
        as_user = ["setpriv", "--reuid=65534", "--regid=65534", "--clear-groups"]
        subprocess.run(["chmod", "-R", "a+rwX", str(work)], check=True)
        os.chmod(work, 0o777)
    uid = 65534 if as_user else os.getuid()
    (run / f"control-{uid}").mkdir()
    if as_user:
        os.chown(run / f"control-{uid}", 65534, 65534)
    # Fichiers du 1er compte, que camille ne peut pas remplacer : dossier
    # partagé fermé en écriture, comme pour un autre propriétaire.
    (run / "action.txt").write_text("message du premier compte\n")
    os.chmod(run / "action.txt", 0o600)
    os.chmod(run, 0o555)
    try:
        result = subprocess.run(
            as_user + ["sh", str(ROOT / "scripts/winlux-control.sh"), "align-center"],
            env={**os.environ, "PATH": f"{work}/bin:/usr/bin:/bin", "WINLUX_CONTROL_ROOT": str(work / "root"),
                 "WINLUX_CONTROL_PER_ACCOUNT": "1", "HOME": str(work / "home"),
                 "XDG_CONFIG_HOME": str(work / "home/.config"), "WINLUX_CONTROL_NO_STATUS": "1",
                 "WINLUX_ROOT_SOCKET": str(work / "absent.sock"), "FAKE_XFCONF": str(store), "FAKE_XFCONF_LOG": str(log)},
            capture_output=True, text=True, timeout=30)
    finally:
        os.chmod(run, 0o755)
    ids = " ".join(json.loads(store.read_text())["/panels/panel-1/plugin-ids"])
    ok(result.returncode == 0, f"camille : align-center réussit malgré action.txt du 1er compte ({result.stderr.strip()[:300]})")
    ok(ids.startswith("3 13 1 15 14 2 8"), f"camille : contrepoids 13 juste après le séparateur 3 (ids {ids})")
    mine = run / f"control-{uid}"
    ok((mine / "action.txt").read_text().strip() == "Opération terminée." if (mine / "action.txt").exists() else False,
       "camille : son message dans /run/winlux/control-<uid>/action.txt")
    ok((run / "action.txt").read_text() == "message du premier compte\n" and not list(run.glob("action.txt.tmp.*")),
       "le message du 1er compte reste, aucun action.txt.tmp.* laissé")
    ok(not (mine / "control.lock").exists(), "verrou du compte libéré")

# Icônes de la barre : un dossier par compte, repli dans ~/.cache.
tray = (ROOT / "scripts/winlux-tray.py").read_text()
ok('Path(f"/run/winlux/tray-{os.getuid()}")' in tray and 'os.access(candidate, os.W_OK)' in tray,
   "winlux-tray : dossier d'icônes par compte, repli si non inscriptible")
with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    blocked = home / "blocked"
    blocked.mkdir()
    env = {k: v for k, v in os.environ.items() if k != "WINLUX_TRAY_DIR"}
    env.update(HOME=str(home), XDG_CACHE_HOME=str(home / "cache"), PATH="/usr/bin:/bin")
    probe = subprocess.run(["python3", "-B", "-c", f"""
import os, runpy, sys
from pathlib import Path
os.environ['WINLUX_TRAY_DIR'] = {str(blocked)!r}
os.chmod({str(blocked)!r}, 0o555)
sys.argv = ['winlux-tray', 'balance']
try:
    runpy.run_path({str(ROOT / 'scripts/winlux-tray.py')!r}, run_name='__main__')
except SystemExit:
    pass
"""], env=env, capture_output=True, text=True, timeout=30)
    os.chmod(blocked, 0o755)
    out = probe.stdout.strip()
    if os.getuid() == 0:
        ok(probe.returncode == 0 and "spacer-0.svg" in out, f"winlux-tray balance sans erreur ({out or probe.stderr[-200:]})")
    else:
        ok(probe.returncode == 0 and f"{home}/cache/winlux/tray/spacer-0.svg" in out,
           f"dossier d'icônes non inscriptible : repli dans ~/.cache, pas de PermissionError ({out or probe.stderr[-300:]})")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("OK: barre d'une 2e session alignée comme la 1re")
