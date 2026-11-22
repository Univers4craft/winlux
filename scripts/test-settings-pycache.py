#!/usr/bin/python3
"""Paramètres : bytecode des pages gardé dans le cache du compte (Gain #3027).

Installés dans /usr/bin (non inscriptible), winlux-settings-pages et les
pages livrées à part étaient recompilés à chaque ouverture. Vérifié ici sur
une copie « installée » (noms sans .py, dossier en lecture seule) :
1. le bytecode est écrit sous $XDG_CACHE_HOME/winlux/pycache, rien à côté ;
2. une page modifiée est relue (jamais de bytecode périmé) ;
3. sans cache inscriptible, les Paramètres se chargent quand même.
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
failures = []


def check(condition, message):
    print(("OK   " if condition else "ÉCHEC ") + message)
    if not condition: failures.append(message)


tmp = Path(tempfile.mkdtemp())
installed, cache = tmp / "bin", tmp / "cache"
installed.mkdir()
for source in HERE.glob("winlux-*.py"):
    shutil.copy(source, installed / source.stem)
PROBE = """import runpy, sys
settings = runpy.run_path(sys.argv[1], run_name="pycache_test")
print("canari=" + str(getattr(settings["EXTRA"], "CANARI", None)))
"""


def load(xdg_cache):
    env = dict(os.environ, XDG_CACHE_HOME=str(xdg_cache), WINLUX_LANG="fr")
    env.pop("PYTHONPYCACHEPREFIX", None); env.pop("PYTHONDONTWRITEBYTECODE", None)
    result = subprocess.run([sys.executable, "-c", PROBE, str(installed / "winlux-settings")],
                            capture_output=True, text=True, env=env, timeout=120)
    print(result.stdout.strip()[-300:], result.stderr.strip()[-600:])
    return result


installed.chmod(0o555)
try:
    first = load(cache)
    cached = sorted(p.name for p in cache.rglob("*.pyc"))
    print("bytecode :", cached)
    check(first.returncode == 0 and "canari=None" in first.stdout, "Paramètres chargés depuis une copie installée")
    check(any(name.startswith("winlux-settings-pages") for name in cached), "bytecode des pages dans le cache du compte")
    check(not (installed / "__pycache__").exists(), "rien écrit à côté des programmes installés")

    installed.chmod(0o755)
    pages = installed / "winlux-settings-pages"
    pages.write_text(pages.read_text() + "\nCANARI = 1\n")
    installed.chmod(0o555)
    check("canari=1" in load(cache).stdout, "page modifiée relue malgré le cache")

    blocked = tmp / "pas-un-dossier"
    blocked.write_text("")
    check("canari=1" in load(blocked).stdout, "chargement sans cache inscriptible")
finally:
    installed.chmod(0o755)
    shutil.rmtree(tmp, ignore_errors=True)
if failures:
    sys.exit("ÉCHEC (#3027) : " + " ; ".join(failures))
print("Paramètres : bytecode des pages dans le cache du compte (#3027)")
