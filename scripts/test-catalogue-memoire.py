#!/usr/bin/python3
"""#3111 : le démon du catalogue garde une mémoire stable de passe en passe,
même avec un gros registre (la passe tourne dans un processus fils)."""
import os
from pathlib import Path
import re
import runpy
import sys
import tempfile

HERE = Path(__file__).resolve().parent

def anon():
    text = Path("/proc/self/smaps_rollup").read_text()
    return int(re.search(r"Pss_Anon:\s+(\d+)", text).group(1)) / 1024

with tempfile.TemporaryDirectory() as folder:
    os.environ["HOME"] = folder
    state = Path(folder) / ".local/share/winlux"
    state.mkdir(parents=True)
    with (state / "registry.tsv").open("w") as registry:
        for i in range(40000):
            key = f"HKLM\\Software\\Classes\\CLSID\\{{{i:08x}-0000}}\\InprocServer32".encode().hex()
            value = f"C:\\Program Files\\App{i % 300}\\lib{i}.dll\0".encode("utf-16-le").hex()
            registry.write(f"V\t{key}\t{f'Value{i % 7}'.encode().hex()}\t1\t{value}\n")
    print("registre :", (state / "registry.tsv").stat().st_size // 1024, "Ko")
    sys.argv = ["winlux-applications", "test"]
    catalogue = runpy.run_path(str(HERE / "winlux-applications.py"), run_name="catalogue_3111")
    passe = catalogue.get("refresh_apart") or (lambda: (catalogue["CHANGED"].clear(), catalogue["refresh"]()))
    passe()
    start = anon()
    values = []
    for _ in range(12):
        passe()
        values.append(round(anon(), 1))
    print(f"mémoire privée après la 1re passe {start:.1f} Mo, puis : {values}")
    assert max(values) - start < 2, f"le démon du catalogue grossit de passe en passe : {start:.1f} → {values}"
    # Une passe qui échoue est journalisée et ne tue pas le démon.
    def boom(): raise RuntimeError("passe en échec (test #3111)")
    catalogue["refresh_apart"].__globals__["refresh"] = boom
    catalogue["refresh_apart"]()
    print("passe en échec : le démon continue")
print("Catalogue, mémoire stable : OK")
