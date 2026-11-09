# Programmes de test PyInstaller et pygame win32 construits par la CI, à
# partir de paquets publics épinglés par sha256 ; aucun .exe dans le dépôt
# (#1346).
ok("--require-hashes" in (ROOT / ".github/workflows/python32.yml").read_text(encoding="utf-8")
   and "architecture: x86" in (ROOT / ".github/workflows/python32.yml").read_text(encoding="utf-8")
   and all("--hash=sha256:" in l for l in (ROOT / "testc/python32/requirements.txt").read_text(encoding="utf-8").split("\\\n")[1:])
   and not list((ROOT / "testc/python32").glob("*.exe")),
   "programmes de test Python win32 épinglés, construits par la CI (#1346)")
