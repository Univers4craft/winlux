#!/usr/bin/env python3
"""#2235 : sous une session où LC_ALL est en français (comptes créés après
l'installation, /etc/default/locale), la Boutique garde le catalogue Ubuntu.
apt choisit la langue de ses messages comme gettext : LC_ALL, sinon
LC_MESSAGES, sinon LANG. Avec LANG=C seul, apt écrivait « Candidat : » et
aucun logiciel de la sélection n'était retenu."""
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

# Langue des messages d'apt, dans l'ordre de gettext.
LANG_OF = 'l=${LC_ALL:-${LC_MESSAGES:-${LANG:-C}}}\ncase "$l" in fr*) fr=1;; *) fr=;; esac\n'

with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    root, home, bindir, run = w / "root", w / "home", w / "bin", w / "run"
    for d in (root, home, bindir, run):
        d.mkdir()
    mock(bindir, "dpkg-query", "printf 'mousepad\\tii \\t0.6.1-1\\t900\\n'\n")
    mock(bindir, "apt-get", "exit 0\n")
    mock(bindir, "apt", LANG_OF + """if [ -n "$fr" ]; then
printf 'En train de lister...\\nmousepad/noble-updates 0.6.2-1 amd64 [pouvant être mis à jour depuis : 0.6.1-1]\\n'
else
printf 'Listing...\\nmousepad/noble-updates 0.6.2-1 amd64 [upgradable from: 0.6.1-1]\\n'
fi
""")
    mock(bindir, "apt-cache", LANG_OF + """case "$1" in
policy) shift; for p in "$@"; do echo "$p:"
  if [ -n "$fr" ]; then echo '  Installé : (aucun)'; echo '  Candidat : 2.9.2-2'
  else echo '  Installed: (none)'; echo '  Candidate: 2.9.2-2'; fi; done;;
rdepends) if [ -n "$fr" ]; then echo 'snapd'; echo 'Dépendances inverses :'; else echo 'snapd'; echo 'Reverse Depends:'; fi
  echo '  firefox';;
esac
""")
    catalog = w / "catalog.txt"
    catalog.write_text("lynx|internet|Lynx|lynx|Lynx|Navigateur texte.|Text browser.|Navegador.|Browser.\n"
                       "mousepad|bureautique|Mousepad|mousepad|Xfce|Éditeur.|Editor.|Editor.|Editor.\n"
                       "firefox|internet|Firefox|firefox|Mozilla|Navigateur.|Browser.|Navegador.|Browser.\n")
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_CONTROL_ROOT": str(root),
           "XDG_CACHE_HOME": str(home / ".cache"), "XDG_DATA_HOME": str(home / ".local/share"),
           "WINLUX_STORE_RUN": str(run), "WINLUX_STORE_OFFLINE": "1", "WINLUX_STORE_CATALOG": str(catalog),
           "WINLUX_LANG": "fr", "WINLUX_STORE_CATALOGS_DEFS": str(w / "aucun.json"),
           # Environnement relevé par le banc dans la Boutique du compte « parent ».
           "LANG": "fr_FR.UTF-8", "LANGUAGE": "fr_FR:en", "LC_ALL": "fr_FR.UTF-8"}
    r = subprocess.run([sys.executable, "-I", "-B", str(store), "refresh"], env=env,
                       capture_output=True, text=True, timeout=300)
    ok(r.returncode == 0, f"refresh se termine ({r.stderr[-300:]!r})")
    index = {e.get("ref"): e for e in json.loads((home / ".cache/winlux/store/index.json").read_text())
             if e.get("source") == "apt"}
    ok("lynx" in index and index["lynx"].get("state") == "a",
       "LC_ALL=fr_FR.UTF-8 : lynx (non installé) reste dans le catalogue Ubuntu (#2235)")
    ok(index.get("mousepad", {}).get("state") == "u" and index["mousepad"].get("available") == "0.6.2-1",
       "LC_ALL=fr_FR.UTF-8 : la mise à jour de Mousepad est vue (#2235)")
    ok("firefox" not in index, "LC_ALL=fr_FR.UTF-8 : le lanceur de snap firefox reste masqué sans snapd (#2235)")
    store_txt = (run / "store.txt").read_text()
    ok(sum(l.startswith("app=apt:") for l in store_txt.splitlines()) == 2,
       "LC_ALL=fr_FR.UTF-8 : store.txt liste lynx et mousepad (#2235)")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique sous une session LC_ALL française : OK")
