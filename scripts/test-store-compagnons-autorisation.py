#!/usr/bin/env python3
"""#2199 : installer un logiciel et ses compagnons ne demande qu'une
autorisation (un seul appel apt), la Boutique dit qu'elle attend une
autorisation au lieu d'un pourcentage figé, et désinstaller retire les
compagnons que l'installation a posés, sauf s'ils servent encore."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import os
project = Path(__file__).resolve().parent.parent
if os.geteuid() == 0:
    sys.exit("test-store-compagnons-autorisation : à lancer sans les droits root (la Boutique passerait outre le relais)")
store = project / "scripts/winlux-store.py"
failures = []

def ok(cond, label):
    print(("OK    " if cond else "ÉCHEC ") + label)
    if not cond:
        failures.append(label)

def mock(bindir, name, body):
    p = bindir / name
    p.write_text("#!/bin/sh\n" + body)
    p.chmod(0o755)

with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    root, home, bindir, run = w / "root", w / "home", w / "bin", w / "run"
    for d in (root, home, bindir, run):
        d.mkdir()
    log, installed = w / "log", w / "installed"
    installed.write_text("")
    catalog = w / "catalog.txt"
    catalog.write_text("knights|jeux|Knights|knights|KDE|Échecs.|Chess.|Ajedrez.|Schach.\n"
                       "compagnons=knights:gnuchess,stockfish\n"
                       "gnome-chess|jeux|Échecs|gnome-chess|GNOME|Échecs.|Chess.|Ajedrez.|Schach.\n"
                       "compagnons=gnome-chess:gnuchess\n")
    # dpkg-query lit la liste des paquets installés tenue par la maquette d'apt-get.
    mock(bindir, "dpkg-query", f"""case "$*" in *Essential*) exit 0;; esac
while read -r p; do [ -n "$p" ] && printf '%s\\tii \\t1\\t10\\n' "$p"; done < "{installed}"
""")
    # apt-get : les simulations (-s) rendent ce que la purge retirerait ;
    # purger stockfish retirerait aussi « analyse-echecs », qui en dépend.
    mock(bindir, "apt-get", f"""printf 'apt-get %s\\n' "$*" >> "{log}"
if [ "$1" = -s ]; then
    case "$*" in *autoremove*) exit 0;; esac
    seen=; for a in "$@"; do
        [ -n "$seen" ] && echo "Purg $a [1]" && [ "$a" = stockfish ] && grep -qx analyse-echecs "{installed}" && echo 'Purg analyse-echecs [1]'
        [ "$a" = -- ] && seen=1
    done; exit 0
fi
fd=; for a in "$@"; do case "$a" in APT::Status-Fd=*) fd=${{a#APT::Status-Fd=}};; esac; done
cat "{run}/store-progress.txt" >> "{w / 'progress.log'}" 2>/dev/null
seen=; verb=; for a in "$@"; do
    case "$a" in install|purge) verb=$a;; --) seen=1; continue;; esac
    [ -z "$seen" ] && continue
    if [ "$verb" = install ]; then echo "$a" >> "{installed}"; else grep -vx "$a" "{installed}" > "{installed}.t"; mv "{installed}.t" "{installed}"; fi
done
[ -n "$fd" ] && eval "echo 'pmstatus:x:60.0:Installation' >&$fd"
exit 0
""")
    # Relais : la demande d'autorisation est simulée, puis la commande passe.
    mock(bindir, "winlux-root", f"""printf 'winlux-root %s\\n' "$*" >> "{log}"
[ "$1" = exec ] && shift && exec "$@"
""")
    # sudo sans mot de passe (CI) court-circuiterait le relais : refusé ici.
    mock(bindir, "sudo", "exit 1\n")
    mock(bindir, "pkexec", "exit 1\n")
    sock = w / "root.sock"
    sock.write_text("")
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_CONTROL_ROOT": str(root),
           "XDG_CACHE_HOME": str(home / ".cache"), "XDG_DATA_HOME": str(home / ".local/share"),
           "WINLUX_STORE_RUN": str(run), "WINLUX_STORE_OFFLINE": "1", "WINLUX_STORE_CATALOG": str(catalog),
           "WINLUX_ROOT_SOCKET": str(sock), "WINLUX_LANG": "fr"}
    mock(bindir, "id", "echo 1004\n")

    def store_op(action, target):
        log.write_text("")
        (home / ".cache/winlux/store").mkdir(parents=True, exist_ok=True)
        (home / ".cache/winlux/store/index.json").write_text(json.dumps(
            [{"source": "apt", "ref": "knights", "name": "Knights", "state": "a"},
             {"source": "apt", "ref": "gnome-chess", "name": "Échecs", "state": "a"}]))
        r = subprocess.run([sys.executable, "-I", "-B", str(store), action, target], env=env,
                           capture_output=True, text=True, timeout=120)
        return r, log.read_text()

    # 1. Installation : un seul appel privilégié, compagnons compris.
    r, journal = store_op("install", "apt:knights")
    relayed = [l for l in journal.splitlines() if l.startswith("winlux-root ")]
    ok(r.returncode == 0 and len(relayed) == 1 and relayed[0].endswith("-- knights gnuchess stockfish"),
       f"une seule commande apt sous autorisation, compagnons compris : {relayed} (#2199)")
    ok("waiting" not in journal and "--mark-auto" not in journal, "compagnons dans le même appel, sans second apt-get (#2199)")
    shown = (w / "progress.log").read_text() if (w / "progress.log").exists() else ""
    ok("En attente de l'autorisation d'un administrateur" in shown and "percent=\n" in shown,
       "la Boutique dit qu'elle attend une autorisation, sans pourcentage figé (#2199)")
    state = json.loads((home / ".local/share/winlux/store-state.json").read_text())
    ok(state.get("apt_companions", {}).get("knights") == ["gnuchess", "stockfish"], "compagnons posés notés (#2199)")

    # 2. Un compagnon déjà présent n'est ni renommé ni noté.
    installed.write_text("gnuchess\n")
    r, journal = store_op("install", "apt:knights")
    relayed = [l for l in journal.splitlines() if l.startswith("winlux-root ")]
    ok(relayed and relayed[0].endswith("-- knights stockfish"), f"compagnon déjà installé laissé tel quel : {relayed} (#2199)")

    # 3. Désinstallation : stockfish sert à « analyse-echecs », il reste ;
    #    gnuchess, posé par l'installation, part avec Knights.
    state = json.loads((home / ".local/share/winlux/store-state.json").read_text())
    state.setdefault("apt_companions", {})["knights"] = ["gnuchess", "stockfish"]
    (home / ".local/share/winlux/store-state.json").write_text(json.dumps(state))
    installed.write_text("knights\ngnuchess\nstockfish\nanalyse-echecs\n")
    r, journal = store_op("remove", "apt:knights")
    purge = [l for l in journal.splitlines() if l.startswith("winlux-root ") and " purge " in l]
    ok(purge and purge[0].endswith("purge -- knights gnuchess"),
       f"désinstallation : compagnons posés retirés, celui dont dépend un autre paquet gardé : {purge} (#2199)")
    left = installed.read_text().split()
    ok("stockfish" in left and "analyse-echecs" in left and "gnuchess" not in left and "knights" not in left,
       f"paquets restants : {left} (#2199)")

    # 4. Compagnon partagé : il reste avec l'autre logiciel, puis part avec lui.
    state = json.loads((home / ".local/share/winlux/store-state.json").read_text())
    state["apt_companions"] = {"knights": ["gnuchess"]}
    (home / ".local/share/winlux/store-state.json").write_text(json.dumps(state))
    installed.write_text("knights\ngnuchess\ngnome-chess\n")
    r, journal = store_op("remove", "apt:knights")
    purge = [l for l in journal.splitlines() if l.startswith("winlux-root ") and " purge " in l]
    ok(purge and purge[0].endswith("purge -- knights"), f"compagnon partagé gardé : {purge} (#2199)")
    state = json.loads((home / ".local/share/winlux/store-state.json").read_text())
    ok(state.get("apt_companions", {}).get("gnome-chess") == ["gnuchess"], "compagnon partagé confié à l'autre logiciel (#2199)")
    r, journal = store_op("remove", "apt:gnome-chess")
    purge = [l for l in journal.splitlines() if l.startswith("winlux-root ") and " purge " in l]
    ok(purge and purge[0].endswith("purge -- gnome-chess gnuchess"), f"puis retiré avec lui : {purge} (#2199)")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique, compagnons sous une seule autorisation : OK")
