#!/usr/bin/python3
"""#1285 : un raccourci du menu Démarrer vers …\\audacity.exe trouve Audacity.exe.

Windows ignore la casse des noms de fichiers : l'installeur d'Audacity crée
Audacity.lnk vers C:\\Program Files\\Audacity\\audacity.exe alors que le
fichier s'appelle Audacity.exe. shortcuts() doit le retrouver, sinon aucun
lanceur winlux-pe-*.desktop n'est créé."""
import importlib.util
from pathlib import Path
import struct
import sys
import tempfile

project = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent)
spec = importlib.util.spec_from_file_location("catalogue", project / "scripts/winlux-applications.py")
catalogue = importlib.util.module_from_spec(spec)
spec.loader.exec_module(catalogue)


def lnk(target):
    data = bytearray(76)
    struct.pack_into("<I", data, 0, 76)
    struct.pack_into("<I", data, 20, 8 | 128)  # chemin relatif, Unicode
    encoded = target.encode("utf-16-le")
    return bytes(data + struct.pack("<H", len(encoded) // 2) + encoded)


def pe():
    data = bytearray(128)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 64)
    data[64:68] = b"PE\0\0"
    return bytes(data)


with tempfile.TemporaryDirectory() as directory:
    home = Path(directory) / "compte"
    exe = home / "Program Files/Audacity/Audacity.exe"
    exe.parent.mkdir(parents=True)
    exe.write_bytes(pe())
    menu = home / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs"
    menu.mkdir(parents=True)
    (menu / "Audacity.lnk").write_bytes(lnk("C:\\program files\\audacity\\audacity.exe"))

    assert catalogue.windows_path("C:\\program files\\AUDACITY\\audacity.exe", home) == exe
    # Fichier absent : la fin du chemin reste telle quelle, la partie trouvée prend la vraie casse.
    assert catalogue.windows_path("C:\\program files\\audacity\\neuf.txt", home) == exe.parent / "neuf.txt"
    assert catalogue.windows_path("C:\\absent\\x.exe", home) == home / "absent/x.exe"
    found = list(catalogue.shortcuts(home))
    assert found == [(exe, "Audacity", [])], found

print("Raccourci en minuscules vers Audacity.exe retrouvé sans tenir compte de la casse (#1285).")
