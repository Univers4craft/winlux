#!/usr/bin/python3
"""Lance un script Python de Winlux depuis son bytecode en cache (#3107).

    winlux-python /usr/bin/winlux-xxx [arguments…]

Un script lancé directement est recompilé depuis la source à chaque démarrage,
et la mémoire du compilateur reste acquise au processus : jusqu'à 10 Mo privés
pour un démon de 3 000 lignes qui tourne toute la session. Ici le code compilé
est relu depuis ~/.cache/winlux/pycache (root : /var/cache/winlux/pycache),
écrit au premier lancement et refait dès que la source change (date et taille).
Le script voit le même __name__ (« __main__ »), le même __file__ et le même
sys.argv que lancé directement ; le nom du processus reste le sien.
"""
import marshal
import os
import sys
from importlib.util import MAGIC_NUMBER


def cache_dir():
    if os.environ.get("WINLUX_PYCACHE"): return os.environ["WINLUX_PYCACHE"]  # tests
    if os.geteuid() == 0: return "/var/cache/winlux/pycache"
    base = os.environ.get("XDG_CACHE_HOME") or os.path.join(os.path.expanduser("~"), ".cache")
    return os.path.join(base, "winlux", "pycache")


def load(path):
    st = os.stat(path)
    stamp = MAGIC_NUMBER + int(st.st_mtime_ns).to_bytes(8, "little") + st.st_size.to_bytes(8, "little")
    cached = os.path.join(cache_dir(), path.lstrip("/").replace("/", "%") + ".pyc")
    try:
        with open(cached, "rb") as handle:
            info = os.fstat(handle.fileno())
            # Seul un cache écrit par ce compte, que personne d'autre ne peut
            # modifier, est exécuté (un service root ne lit que le sien).
            if info.st_uid == os.geteuid() and not info.st_mode & 0o022:
                data = handle.read()
                if data[:len(stamp)] == stamp: return marshal.loads(data[len(stamp):])
    except (OSError, ValueError, EOFError, TypeError):
        pass
    with open(path, "rb") as handle:
        code = compile(handle.read(), path, "exec", dont_inherit=True)
    try:
        os.makedirs(os.path.dirname(cached), mode=0o700, exist_ok=True)
        temporary = f"{cached}.{os.getpid()}"
        with open(os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb") as handle:
            handle.write(stamp + marshal.dumps(code))
        os.replace(temporary, cached)
    except OSError:
        pass  # cache illisible ou plein : le script tourne quand même
    return code


def main():
    if len(sys.argv) < 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    path = os.path.abspath(sys.argv[1])
    code = load(path)
    try:
        with open("/proc/self/comm", "w") as comm:
            comm.write(os.path.basename(path)[:15])
    except OSError:
        pass
    sys.argv = sys.argv[1:]
    sys.path[0] = os.path.dirname(path)
    module = type(sys)("__main__")
    module.__file__ = path
    module.__builtins__ = __builtins__
    sys.modules["__main__"] = module
    exec(code, module.__dict__)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
