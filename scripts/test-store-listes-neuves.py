#!/usr/bin/env python3
"""#2184 : sur un système neuf (image sans listes apt), la Boutique lance
apt-get update d'elle-même, sans autorisation, pour montrer les logiciels
des dépôts dès la première ouverture ; jamais hors ligne, et pas quand les
listes sont là et récentes."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
store = project / "scripts/winlux-store.py"
failures = []
if os.geteuid() == 0:
    sys.exit("test-store-listes-neuves : à lancer sans les droits root (la Boutique passerait outre le relais)")

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
    lists = root / "var/lib/apt/lists"
    lists.mkdir(parents=True)
    log = w / "log"
    catalog = w / "catalog.txt"
    catalog.write_text("lynx|internet|Lynx|lynx|Lynx|Navigateur.|Browser.|Navegador.|Browser.\n")
    mock(bindir, "dpkg-query", "exit 0\n")
    mock(bindir, "apt", "exit 0\n")
    # apt-cache : candidat seulement une fois les listes téléchargées.
    mock(bindir, "apt-cache", f"""case "$1" in policy) shift; for p in "$@"; do echo "$p:"
  if ls "{lists}"/*_Packages >/dev/null 2>&1; then echo '  Candidate: 2.9'; else echo '  Candidate: (none)'; fi; done;; esac
""")
    mock(bindir, "apt-get", f"""printf 'apt-get %s\\n' "$*" >> "{log}"
case " $* " in *" update "*) : > "{lists}/archive.ubuntu.com_ubuntu_dists_noble_main_binary-amd64_Packages";; esac
""")
    mock(bindir, "winlux-root", f"""printf 'winlux-root %s\\n' "$*" >> "{log}"
[ "$1" = exec ] && shift && exec "$@"
""")
    mock(bindir, "sudo", "exit 1\n")
    mock(bindir, "pkexec", "exit 1\n")
    mock(bindir, "winlux-uac", f"printf 'uac\\n' >> \"{log}\"\nexit 0\n")
    sock = w / "root.sock"
    sock.write_text("")
    routes = w / "route"
    routes.write_text("Iface\tDestination\nenp1s0\t00000000\n")
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_CONTROL_ROOT": str(root),
           "XDG_CACHE_HOME": str(home / ".cache"), "XDG_DATA_HOME": str(home / ".local/share"),
           "WINLUX_STORE_RUN": str(run), "WINLUX_STORE_CATALOG": str(catalog),
           "WINLUX_ROOT_SOCKET": str(sock), "WINLUX_LANG": "fr", "WINLUX_NETWORK_ROUTES": str(routes),
           "WINLUX_STORE_CATALOGS_DEFS": str(w / "aucun.json")}

    def refresh(**extra):
        log.write_text("")
        r = subprocess.run([sys.executable, "-I", "-B", str(store), "refresh"], env={**env, **extra},
                           capture_output=True, text=True, timeout=300)
        updates = [l for l in log.read_text().splitlines() if l.startswith("apt-get ") and l.endswith(" update")]
        return r, updates, log.read_text(), (run / "store.txt").read_text()

    # Hors ligne (simulé, puis sans route par défaut) : rien n'est lancé.
    r, updates, journal, txt = refresh(WINLUX_STORE_OFFLINE="1")
    ok(r.returncode == 0 and not updates, "hors ligne : pas d'apt-get update (#2184)")
    routes.write_text("Iface\tDestination\nenp1s0\t0000A8C0\n")
    r, updates, journal, txt = refresh()
    ok(not updates, "sans route par défaut : pas d'apt-get update (#2184)")
    routes.write_text("Iface\tDestination\nenp1s0\t00000000\n")

    # Système neuf, en ligne : un seul apt-get update, par le relais, sans UAC.
    r, updates, journal, txt = refresh()
    ok(len(updates) == 1 and "winlux-root exec apt-get" in journal and "uac" not in journal,
       f"listes absentes : apt-get update lancé une fois par le relais, sans autorisation ({updates}) (#2184)")
    ok("app=apt:lynx|" in txt, "les logiciels des dépôts apparaissent sans « Actualiser » (#2184)")

    # Listes présentes et récentes : rien de plus.
    r, updates, journal, txt = refresh()
    ok(not updates and "app=apt:lynx|" in txt, "listes récentes : pas de nouvel apt-get update (#2184)")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique, listes apt d'un système neuf : OK")
