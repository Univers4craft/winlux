#!/usr/bin/env python3
"""#2183 : LibreOffice s'installe avec ses traductions française, allemande
et espagnole (libreoffice-l10n-*, que rien ne recommande), et désinstaller
Writer ne les retire pas à Calc."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import os
project = Path(__file__).resolve().parent.parent
if os.geteuid() == 0:
    sys.exit("test-store-libreoffice-traductions : à lancer sans les droits root (la Boutique passerait outre le relais)")
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
    catalog = project / "integration/store-catalog.txt"
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
            [{"source": "apt", "ref": f"libreoffice-{a}", "name": f"LibreOffice {a}", "state": "a"}
             for a in ("writer", "calc", "impress", "draw", "base", "math")]))
        r = subprocess.run([sys.executable, "-I", "-B", str(store), action, target], env=env,
                           capture_output=True, text=True, timeout=120)
        return r, log.read_text()

    L10N = "libreoffice-l10n-fr libreoffice-l10n-de libreoffice-l10n-es"
    for app in ("writer", "calc", "impress", "draw", "math"):
        installed.write_text("")
        r, journal = store_op("install", f"apt:libreoffice-{app}")
        relayed = [l for l in journal.splitlines() if l.startswith("winlux-root ")]
        ok(relayed and relayed[0].endswith(f"-- libreoffice-{app} {L10N}"),
           f"LibreOffice {app} installé avec ses traductions fr/de/es : {relayed[-1:]} (#2183)")

    # Writer puis Calc : les traductions ne sont posées qu'une fois.
    installed.write_text("")
    (home / ".local/share/winlux/store-state.json").unlink(missing_ok=True)
    store_op("install", "apt:libreoffice-writer")
    r, journal = store_op("install", "apt:libreoffice-calc")
    relayed = [l for l in journal.splitlines() if l.startswith("winlux-root ")]
    ok(relayed and relayed[0].endswith("-- libreoffice-calc"), f"traductions déjà là, non renommées : {relayed} (#2183)")
    # Désinstaller Writer : Calc garde ses traductions.
    r, journal = store_op("remove", "apt:libreoffice-writer")
    left = installed.read_text().split()
    ok(all(f"libreoffice-l10n-{l}" in left for l in ("fr", "de", "es")) and "libreoffice-writer" not in left,
       f"Writer désinstallé, Calc garde les traductions : {left} (#2183)")
    # Puis Calc : les traductions partent avec le dernier logiciel LibreOffice.
    r, journal = store_op("remove", "apt:libreoffice-calc")
    left = installed.read_text().split()
    ok(not any(p.startswith("libreoffice-") for p in left), f"Calc désinstallé, plus rien de LibreOffice : {left} (#2183)")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique, traductions de LibreOffice : OK")
