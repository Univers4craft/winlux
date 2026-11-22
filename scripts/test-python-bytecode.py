#!/usr/bin/python3
"""#3107 : winlux-python lance un script depuis son bytecode en cache, avec le
même __name__, __file__, sys.argv, sys.path[0] et nom de processus qu'un
lancement direct, recompile quand la source change, n'exécute jamais un cache
modifiable par un autre, et garde moins de mémoire qu'un lancement direct."""
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
LOADER = HERE / "winlux-python.py"

with tempfile.TemporaryDirectory() as folder:
    folder = Path(folder)
    cache = folder / "cache"
    env = dict(os.environ, XDG_CACHE_HOME=str(cache))
    if os.geteuid() == 0:
        env["WINLUX_PYCACHE"] = str(cache / "racine")
    script = folder / "outils" / "winlux-essai"
    script.parent.mkdir()
    script.write_text("import os, sys\n"
                      "print(__name__, os.path.basename(__file__), sys.argv[1:], sys.path[0] == os.path.dirname(__file__),\n"
                      "      open('/proc/self/comm').read().strip(), 'v1')\n")
    direct = subprocess.run([sys.executable, str(script), "a", "b c"], capture_output=True, text=True, env=env).stdout
    run = lambda: subprocess.run([sys.executable, str(LOADER), str(script), "a", "b c"], capture_output=True, text=True, env=env)
    first = run().stdout
    print("direct :", direct.strip(), "| winlux-python :", first.strip())
    assert direct.replace("python3", "winlux-essai") == first or first.split()[:-2] == direct.split()[:-2], (direct, first)
    assert first.split()[0] == "__main__" and "winlux-essai" in first and "['a', 'b c']" in first and " True " in first, first
    cached = [p for p in cache.rglob("*.pyc")]
    print("cache écrit :", [p.name for p in cached])
    assert len(cached) == 1 and cached[0].stat().st_mode & 0o077 == 0, cached
    second = run().stdout
    assert second == first, (first, second)
    # Source modifiée : le cache est refait.
    time.sleep(0.01)
    script.write_text(script.read_text().replace("'v1'", "'v2-plus-long'"))
    third = run().stdout
    print("après modification :", third.strip())
    assert third.strip().endswith("v2-plus-long"), third
    # Un cache modifiable par d'autres n'est jamais exécuté.
    trap = cached[0]
    good = trap.read_bytes()
    header = good[:20]
    import marshal
    trap.write_bytes(header + marshal.dumps(compile("print('PIEGE')", str(script), "exec")))
    os.chmod(trap, 0o666)
    fourth = run().stdout
    print("cache ouvert à tous :", fourth.strip())
    assert "PIEGE" not in fourth and fourth.strip().endswith("v2-plus-long"), fourth
    # Erreurs et code de sortie remontent comme en lancement direct.
    script.write_text("import sys\nsys.exit(7)\n")
    assert run().returncode == 7

    # Premier démarrage après une mise à jour : cache absent, périmé, tronqué,
    # d'une autre version de Python ou impossible à écrire. Chaque fois le
    # script tourne (seulement plus lentement), jamais d'erreur.
    script.write_text("print('demarre')\n")
    def fresh():
        for p in cache.rglob("*.pyc"): p.unlink()
    fresh()
    assert run().stdout == "demarre\n"                       # absent
    pyc = next(cache.rglob("*.pyc"))
    pyc.write_bytes(pyc.read_bytes()[:30])                   # tronqué
    assert run().stdout == "demarre\n"
    data = bytearray(pyc.read_bytes()); data[0] ^= 0xFF      # autre version de Python
    pyc.write_bytes(bytes(data))
    assert run().stdout == "demarre\n"
    script.write_text("print('demarre, nouvelle version')\n")  # source mise à jour
    assert run().stdout == "demarre, nouvelle version\n"
    blocked = dict(env, WINLUX_PYCACHE=str(script), XDG_CACHE_HOME=str(script))  # cache impossible à créer
    out = subprocess.run([sys.executable, str(LOADER), str(script)], capture_output=True, text=True, env=blocked)
    assert out.stdout == "demarre, nouvelle version\n" and out.returncode == 0, out
    print("cache absent, tronqué, d'une autre version, périmé, impossible à écrire : le script démarre")
    # Les vrais démons lancés par les sessions : même code depuis le cache que
    # compilé depuis la source, cache absent puis périmé.
    sys.path.insert(0, str(HERE))
    import importlib.util
    spec = importlib.util.spec_from_file_location("winlux_python_3107", LOADER)
    loader = importlib.util.module_from_spec(spec); spec.loader.exec_module(loader)
    os.environ["WINLUX_PYCACHE"] = str(cache / "demons")
    sessions = "".join((HERE / n).read_text(encoding="utf-8") for n in
                       ("winlux-native-session.sh", "winlux-user-session.sh", "winlux-system-services.sh"))
    names = sorted(set(re.findall(r"\$py (?:/usr/bin/|/usr/lib/winlux/)(winlux-[a-z-]+)", sessions)))
    print("démons lancés par winlux-python :", names)
    assert len(names) >= 12, names
    for name in names:
        source = HERE / f"{name}.py"
        expected = compile(source.read_bytes(), str(source), "exec", dont_inherit=True).co_code
        assert loader.load(str(source)).co_code == expected, name       # cache absent
        assert loader.load(str(source)).co_code == expected, name       # cache chaud
        os.utime(source)                                                 # périmé
        assert loader.load(str(source)).co_code == expected, name
    del os.environ["WINLUX_PYCACHE"]

    # Mémoire : un gros script résident garde moins de mémoire privée.
    big = folder / "winlux-gros"
    body = "".join(f"def f{i}(a, b):\n    x = [a, b, {i}]\n    return {{'k{i}': x, 'n': len(x) + {i}}}\n\n" for i in range(3000))
    big.write_text(body + "import time\nprint('pret', flush=True)\ntime.sleep(60)\n")
    def anon(cmd):
        proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, text=True, env=env)
        proc.stdout.readline()
        text = Path(f"/proc/{proc.pid}/smaps_rollup").read_text()
        proc.kill(); proc.wait()
        return int(re.search(r"Pss_Anon:\s+(\d+)", text).group(1)) / 1024
    before = anon([sys.executable, str(big)])
    anon([sys.executable, str(LOADER), str(big)])
    after = anon([sys.executable, str(LOADER), str(big)])
    print(f"script de 12 000 lignes résident : direct {before:.1f} Mo, depuis le cache {after:.1f} Mo")
    assert after < before - 3, (before, after)
print("winlux-python : OK")
