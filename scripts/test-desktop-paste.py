#!/usr/bin/python3
"""Bureau > Coller : lecture du presse-papiers Linux et collage local (sans X11)."""
import importlib.util
import tempfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("paste", Path(__file__).resolve().parent / "winlux-desktop-paste.py")
paste = importlib.util.module_from_spec(spec)
spec.loader.exec_module(paste)

paths, cut = paste.parse_clipboard("cut\nfile:///root/T%C3%A9l%C3%A9chargements/Setup.exe\nfile://autre-pc/x")
assert cut and paths == [Path("/root/Téléchargements/Setup.exe")], (paths, cut)
paths, cut = paste.parse_clipboard("# commentaire\r\nfile:///tmp/a%20b.txt\r\n")
assert not cut and paths == [Path("/tmp/a b.txt")], (paths, cut)
assert paste.parse_clipboard("") == ([], False)

with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    downloads, desktop = root / "Téléchargements", root / "Bureau"
    downloads.mkdir(); desktop.mkdir()
    (downloads / "Setup.exe").write_bytes(b"MZ")
    (downloads / "dossier").mkdir(); (downloads / "dossier" / "f.txt").write_text("x")
    (desktop / "Setup.exe").write_bytes(b"old")
    # Copier : la source reste, un nom déjà pris devient « (2) ».
    assert paste.local_paste([downloads / "Setup.exe", downloads / "dossier"], desktop, cut=False) == 0
    assert (downloads / "Setup.exe").exists() and (desktop / "Setup (2).exe").read_bytes() == b"MZ"
    assert (desktop / "dossier" / "f.txt").read_text() == "x"
    # Couper : déplacement ; un dossier ne se copie pas dans lui-même.
    assert paste.local_paste([downloads / "Setup.exe"], desktop, cut=True) == 0
    assert not (downloads / "Setup.exe").exists() and (desktop / "Setup (3).exe").exists()
    assert paste.local_paste([desktop / "dossier"], desktop / "dossier", cut=False) == 1
print("winlux-desktop-paste : OK")
