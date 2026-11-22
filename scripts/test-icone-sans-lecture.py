#!/usr/bin/python3
"""#3164 : extraire l'icône d'un gros .exe ne lit que ses en-têtes et ses
ressources, sans le charger en mémoire ; mêmes icônes qu'avant."""
import importlib.util
import os
from pathlib import Path
import re
import struct
import sys
import tempfile

HERE = Path(__file__).resolve().parent
source = (HERE / "test-native-catalogue.py").read_text(encoding="utf-8")
namespace = {"struct": struct}
exec(source[source.index("def fixture"):source.index("with tempfile.TemporaryDirectory")], namespace)
spec = importlib.util.spec_from_file_location("catalogue_3164", HERE / "winlux-applications.py")
catalogue = importlib.util.module_from_spec(spec)
sys.argv = ["winlux-applications", "test"]
spec.loader.exec_module(catalogue)

def counters():
    io = Path("/proc/self/io").read_text()
    status = Path("/proc/self/status").read_text()
    return int(re.search(r"rchar: (\d+)", io).group(1)), int(re.search(r"VmHWM:\s+(\d+)", status).group(1))

with tempfile.TemporaryDirectory() as folder:
    small = bytes(namespace["fixture"]())
    (Path(folder) / "petit.exe").write_bytes(small)
    big = Path(folder) / "installateur.exe"
    with big.open("wb") as stream:
        stream.write(small)
        stream.truncate(300 * 1024 * 1024)  # 300 Mo, creux : au-delà de l'ancienne limite de 256 Mo
    expected = catalogue.pe_icon(Path(folder) / "petit.exe")
    assert expected and expected[:6] == struct.pack("<HHH", 0, 1, 1), expected
    read0, peak0 = counters()
    icon = catalogue.pe_icon(big)
    read1, peak1 = counters()
    print(f".exe de 300 Mo : icône {len(icon or b'')} octets, {(read1 - read0) / 1024:.0f} Ko lus, pic {peak0 // 1024} → {peak1 // 1024} Mo")
    assert icon == expected, "même icône qu'un petit .exe"
    assert read1 - read0 < 1024 * 1024, "seuls les en-têtes et les ressources sont lus"
    assert peak1 - peak0 < 32 * 1024, "aucun pic de mémoire à la taille du fichier"
    empty = Path(folder) / "vide.exe"; empty.write_bytes(b"")
    assert catalogue.pe_icon(empty) is None
    text = Path(folder) / "texte.exe"; text.write_bytes(b"pas un PE" * 20)
    assert catalogue.pe_icon(text) is None
print("Icône d'un .exe sans lecture entière : OK")
