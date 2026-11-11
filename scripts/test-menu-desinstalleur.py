#!/usr/bin/python3
"""#1748 : le menu Démarrer n'affiche pas « Uninstall » seul.

L'installeur de WinRAR pose Programmes\\WinRAR\\WinRAR.lnk et
Programmes\\WinRAR\\Uninstall.lnk. Le menu aplatit les dossiers : le
désinstalleur est écarté (la désinstallation passe par « Applications
installées ») et un raccourci au nom générique garde le nom de son dossier."""
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
    app = home / "Program Files/WinRAR"
    app.mkdir(parents=True)
    for name in ("WinRAR.exe", "Uninstall.exe", "unins000.exe", "Outil.exe"):
        (app / name).write_bytes(pe())
    (app / "WhatsNew.txt").write_text("7.23")
    menu = home / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs"
    (menu / "WinRAR").mkdir(parents=True)
    (menu / "WinRAR/WinRAR.lnk").write_bytes(lnk("C:\\Program Files\\WinRAR\\WinRAR.exe"))
    (menu / "WinRAR/Uninstall.lnk").write_bytes(lnk("C:\\Program Files\\WinRAR\\Uninstall.exe"))
    # Nom libre, mais la cible est un désinstalleur Inno Setup.
    (menu / "WinRAR/Supprimer WinRAR.lnk").write_bytes(lnk("C:\\Program Files\\WinRAR\\unins000.exe"))
    (menu / "WinRAR/Aide.lnk").write_bytes(lnk("C:\\Program Files\\WinRAR\\Outil.exe"))
    (menu / "WinRAR/What's new.lnk").write_bytes(lnk("C:\\Program Files\\WinRAR\\WhatsNew.txt"))
    (menu / "Outil.lnk").write_bytes(lnk("C:\\Program Files\\WinRAR\\Outil.exe"))

    found = sorted((name, target.name) for target, name, _ in catalogue.shortcuts(home))
    assert found == [("Outil", "Outil.exe"), ("WinRAR", "WinRAR.exe"), ("WinRAR › Aide", "Outil.exe")], found
    links = [name for name, _, _ in catalogue.link_shortcuts(home)]
    assert links == ["WinRAR › What's new"], links

    for name in ("Uninstall", "Uninstall WinRAR", "Désinstaller VLC", "Deinstallieren", "Desinstalar", "unins001"):
        assert catalogue.is_uninstaller(name), name
    for name in ("WinRAR", "Universal Installer", "Unity Hub", "Install fonts"):
        assert not catalogue.is_uninstaller(name), name

    # DisplayIcon vers le désinstalleur : pas de lanceur qui désinstallerait.
    registry = {"hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\winrar": {
        "displayname": "WinRAR 7.23 (64-bit)", "displayicon": "C:\\Program Files\\WinRAR\\Uninstall.exe,0"},
        "hklm\\software\\microsoft\\windows\\currentversion\\uninstall\\outil": {
        "displayname": "Outil", "displayicon": "C:\\Program Files\\WinRAR\\Outil.exe"}}
    entries = [(target.name, name) for target, name in catalogue.installed_registry(home, registry)]
    assert entries == [("Outil.exe", "Outil")], entries

    # #1810 : un désinstalleur déjà lancé (launchers.json) ne redonne pas d'entrée.
    state = home / ".local/share/winlux"
    state.mkdir(parents=True)
    import json
    (state / "launchers.json").write_text(json.dumps([
        {"path": str(app / "Uninstall.exe"), "name": "Uninstall"},
        {"path": str(app / "WinRAR.exe"), "name": "WinRAR"}]))
    found = [(target.name, name) for target, name in catalogue.launched(home)]
    assert found == [("WinRAR.exe", "WinRAR")], found

print("Menu Démarrer : désinstalleurs écartés, noms génériques préfixés du dossier (#1748).")
