#!/usr/bin/env python3
"""#2234 : désinstaller depuis la Boutique un paquet dont dépend un paquet
essentiel ou protégé (espeak-ng, requis par winlux-desktop) ne retire rien,
ne demande aucune autorisation et dit pourquoi, dans les quatre langues.
Avant : « Échec (100) : voir /run/winlux/store-<uid>/store.log »."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
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

# Sortie réelle d'apt 2.8 (LC_ALL=C) pour un paquet requis par un paquet
# Protected: yes, relevée sur des paquets factices installés avec dpkg.
UNMET = """NOTE: This is only a simulation!
Reading package lists...
Building dependency tree...
Reading state information...
Some packages could not be installed. This may mean that you have
requested an impossible situation or if you are using the unstable
distribution that some required packages have not yet been created
or been moved out of Incoming.
The following information may help to resolve the situation:

The following packages have unmet dependencies:
 winlux-desktop : Depends: espeak-ng but it is not going to be installed
E: Error, pkgProblemResolver::Resolve generated breaks, this may be caused by held packages.
"""

with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    root, home, bindir, run = w / "root", w / "home", w / "bin", w / "run"
    for d in (root, home, bindir, run):
        d.mkdir()
    log = w / "log"
    (w / "unmet.txt").write_text(UNMET)
    # apt-get : la simulation (-s) échoue comme apt réel pour espeak-ng et
    # réussit pour lynx ; toute exécution réelle est journalisée.
    mock(bindir, "apt-get", f"""printf 'apt-get %s\\n' "$*" >> "{log}"
if [ "$1" = -s ]; then
    case "$*" in *" -- espeak-ng") cat "{w / 'unmet.txt'}"; echo 'E: Error' >&2; exit 100;; esac
    echo 'Purg lynx [2.9.0]'; exit 0
fi
exit 0
""")
    mock(bindir, "dpkg-query", f"""printf 'dpkg-query %s\\n' "$*" >> "{log}"
for p in "$@"; do case "$p" in
winlux-desktop) printf 'winlux-desktop\\tyes\\tyes\\n';;
espeak-ng|lynx) printf '%s\\tno\\tno\\n' "$p";;
esac; done
""")
    for tool in ("sudo", "pkexec", "winlux-root"):
        mock(bindir, tool, f"printf '{tool} %s\\n' \"$*\" >> \"{log}\"\nexit 0\n")
    mock(bindir, "id", "echo 1004\n")
    cache = home / ".cache/winlux/store"
    cache.mkdir(parents=True)
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_CONTROL_ROOT": str(root),
           "XDG_CACHE_HOME": str(home / ".cache"), "XDG_DATA_HOME": str(home / ".local/share"),
           "WINLUX_STORE_RUN": str(run), "WINLUX_STORE_OFFLINE": "1",
           "WINLUX_ROOT_SOCKET": str(w / "absent.sock")}

    def remove(ref, lang):
        log.write_text("")
        # Index réécrit à chaque fois : la Boutique le recalcule après chaque opération.
        (cache / "index.json").write_text(json.dumps([
            {"source": "apt", "ref": "espeak-ng", "name": "eSpeak NG", "state": "i"},
            {"source": "apt", "ref": "lynx", "name": "Lynx", "state": "i"}]))
        r = subprocess.run([sys.executable, "-I", "-B", str(store), "remove", f"apt:{ref}"],
                           env={**env, "WINLUX_LANG": lang}, capture_output=True, text=True, timeout=120)
        progress = dict(l.split("=", 1) for l in (run / "store-progress.txt").read_text().splitlines() if "=" in l)
        return r, progress, log.read_text()

    expected = {"fr": "fait partie de Winlux", "en": "is part of Winlux",
                "es": "forma parte de Winlux", "de": "ist Teil von Winlux"}
    for lang, words in expected.items():
        r, progress, journal = remove("espeak-ng", lang)
        message = progress.get("message", "")
        ok(progress.get("state") == "error" and words in message and "winlux-desktop" in message
           and "eSpeak NG" in message, f"{lang} : refus expliqué, nomme le logiciel et winlux-desktop ({message!r}) (#2234)")
        ok("store.log" not in message and "(100)" not in message, f"{lang} : plus de « Échec (100) : voir store.log » (#2234)")
        real = [l for l in journal.splitlines() if l.startswith("apt-get ") and not l.startswith("apt-get -s ")]
        ok(not real and not any(l.startswith(("sudo ", "pkexec ", "winlux-root ")) for l in journal.splitlines()),
           f"{lang} : rien n'est exécuté et aucune autorisation n'est demandée (#2234)")
        ok(r.returncode == 11, f"{lang} : code 11, paquet nécessaire au système (#2234)")

    # Un paquet libre se désinstalle toujours, après la simulation.
    r, progress, journal = remove("lynx", "fr")
    ok("purge --autoremove -- lynx" in journal.replace("apt-get -s -q purge", ""),
       "un paquet sans dépendant protégé est bien purgé (#2234)")
    ok(progress.get("state") == "done", f"désinstallation de lynx terminée ({progress.get('message')!r}) (#2234)")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique, paquet nécessaire au système : OK")
