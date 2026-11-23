#!/usr/bin/python3
"""Bureau au repos : les petites boucles ne lancent plus de programmes (#3130).

Vrai PC, 22 novembre : 114 programmes lancés par seconde au repos. En plus
de #3126 et #3129, plusieurs boucles relançaient des programmes à chaque tour :

1. winlux-clock (greffon genmon, toutes les 2 s) : sed, head, tr, cut et
   sous-shells, environ quinze programmes → un seul, date.
2. winlux-input-watch (toutes les 2 s) : grep, xinput, cksum → aucun au repos ;
   la liste de X n'est relue qu'après un changement du noyau.
3. winlux-network-watch (root, toutes les 10 s) : ip | grep et
   winlux-network-perms (une vingtaine de programmes) → route lue dans /proc,
   perms seulement si un fichier qu'il corrige a été réécrit.
4. Boucles de session toutes les 5 s : « $(id -u) » lu une fois.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
failures = []


def check(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


with tempfile.TemporaryDirectory() as directory:
    tmp = Path(directory)
    log = tmp / "lancés.log"
    stubs = tmp / "bin"
    stubs.mkdir()
    # Chaque programme externe appelé laisse son nom, puis fait son vrai travail.
    for name in ("date", "sed", "head", "tr", "cut", "dirname", "grep", "cksum", "cat", "ip", "id", "stat"):
        real = shutil.which(name)
        if real:
            (stubs / name).write_text(f'#!/bin/sh\necho {name} >>"{log}"\nexec {real} "$@"\n')
            (stubs / name).chmod(0o755)
    env = dict(os.environ, PATH=f"{stubs}:/usr/bin:/bin", HOME=str(tmp), XDG_CONFIG_HOME=str(tmp / "config"))
    (tmp / "config/gtk-3.0").mkdir(parents=True)
    (tmp / "config/gtk-3.0/settings.ini").write_text("[Settings]\ngtk-theme-name = Winlux-Violet-Dark\n")

    # 1. Horloge
    result = subprocess.run(["sh", str(HERE / "winlux-clock.sh")], env=env, capture_output=True, text=True)
    launched = log.read_text().split() if log.exists() else []
    print("horloge : programmes lancés", launched)
    check(launched == ["date"], "horloge : un seul programme par tour, date (#3130)")
    check("<txt>" in result.stdout and "#f3f0fa" in result.stdout,
          "horloge : texte clair sur barre sombre (thème Winlux-…-Dark, casse ignorée)")
    (tmp / "config/gtk-3.0/settings.ini").write_text("[Settings]\ngtk-theme-name=winlux-violet\n")
    light = subprocess.run(["sh", str(HERE / "winlux-clock.sh")], env=env, capture_output=True, text=True).stdout
    check("#1d1a24" in light, "horloge : texte foncé sur barre claire")
    env_en = dict(env, WINLUX_LANG="en_US.UTF-8")
    english = subprocess.run(["sh", str(HERE / "winlux-clock.sh")], env=env_en, capture_output=True, text=True).stdout
    check(any(day in english for day in ("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday")),
          "horloge : jour de la semaine en anglais")

    # 2. Surveillance des entrées : au repos, ni xinput ni grep ni cksum.
    root = tmp / "racine"
    (root / "proc/bus/input").mkdir(parents=True)
    (root / "etc/winlux").mkdir(parents=True)
    (root / "etc/winlux/input.conf").write_text("XKB_LAYOUT=fr\n")
    (root / "proc/bus/input/devices").write_text("I: Bus=0011\nN: Name=\"AT Translated Set 2 keyboard\"\nH: Handlers=sysrq kbd event0\n\n")
    (stubs / "xinput").write_text(f'#!/bin/sh\necho xinput >>"{log}"\nprintf "2\\n3\\n"\n')
    (stubs / "xinput").chmod(0o755)
    (stubs / "setxkbmap").write_text(f'#!/bin/sh\necho setxkbmap >>"{log}"\n')
    (stubs / "setxkbmap").chmod(0o755)
    watch_env = dict(env, WINLUX_INPUT_ROOT=str(root), WINLUX_INPUT_INTERVAL="0.1",
                     XDG_RUNTIME_DIR=str(tmp), DISPLAY=":97")
    watcher = subprocess.Popen(["sh", str(HERE / "winlux-input-watch.sh")], env=watch_env)
    try:
        time.sleep(0.5)
        log.write_text("")
        time.sleep(1.5)  # 15 tours au repos
        idle = [name for name in log.read_text().split() if name != "sleep"]
        print("entrées au repos : programmes lancés", idle)
        check(idle == [], "surveillance des entrées au repos : aucun programme lancé (#3130)")
        with open(root / "proc/bus/input/devices", "a") as devices:
            devices.write("N: Name=\"USB Keyboard\"\nH: Handlers=kbd event5\n\n")
        for _ in range(40):
            if "setxkbmap" in log.read_text():
                break
            time.sleep(0.1)
        check("setxkbmap" in log.read_text(), "clavier branché : disposition réappliquée")
    finally:
        watcher.terminate()
        watcher.wait(timeout=5)

    # 3. Surveillance réseau : route par défaut sans ip, perms seulement si besoin.
    source = (HERE / "winlux-network-watch.sh").read_text()
    start = source.index("perms() {")
    end = source.index("\nrounds=0\n")
    functions = source[start:end]
    mark = tmp / "perms.done"
    watched = tmp / "resolv.conf"
    watched.write_text("nameserver 1.1.1.1\n")
    functions = functions.replace("/run/winlux-network-perms.done", str(mark))
    functions = functions.replace("for watched in /etc/resolv.conf", f"for watched in {watched}")
    perms_log = tmp / "perms.log"
    script = (functions + "\n" + f"perms() {{ echo passe >>{perms_log}; }}\n"
              + "rounds=0\n: >\"$done_mark\"\n"
              + "for i in 1 2 3 4 5; do perms_if_due; done\n"
              + f"sleep 1.1; echo 'nameserver 9.9.9.9' >{watched}\n"
              + "for i in 1 2 3; do perms_if_due; done\n"
              + "rounds=59; perms_if_due\n"
              + "has_default_route && echo ROUTE=oui || echo ROUTE=non\n")
    log.write_text("")
    result = subprocess.run(["sh", "-c", script], env=env, capture_output=True, text=True)
    passes = perms_log.read_text().split() if perms_log.exists() else []
    print("perms : passes", len(passes), result.stderr.strip())
    check(len(passes) == 2, "réseau : perms une fois après réécriture de resolv.conf, une fois au bout de 10 min, jamais sinon")
    # Sans ip : la table du noyau, lue par Python, fait foi.
    rows = [line.split() for line in Path("/proc/net/route").read_text().splitlines()[1:]]
    expected = "oui" if any(len(r) > 7 and r[1] == "00000000" and r[7] == "00000000" for r in rows) else "non"
    if shutil.which("ip"):
        routes = subprocess.run(["ip", "-4", "route", "show", "default"], capture_output=True, text=True).stdout
        expected = "oui" if routes.startswith("default ") else "non"
    check(f"ROUTE={expected}" in result.stdout, f"route par défaut lue dans /proc comme ip ({expected})")
    launched = [name for name in log.read_text().split() if name not in ("sleep",)]
    check("ip" not in launched and "grep" not in launched, "réseau : ni ip ni grep à chaque tour")

    # 4. Boucles de session.
    for script_name in ("winlux-user-session.sh", "winlux-native-session.sh"):
        text = (HERE / script_name).read_text()
        loop = text[text.index("browser-chooser-"):text.index("browser-pending-") + 80]
        check("$(id -u)" not in loop, f"{script_name} : compte lu une fois, pas à chaque tour (#3130)")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("boucles au repos : OK")
