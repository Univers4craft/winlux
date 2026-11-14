# #2400 : la consigne du programme (lpszTitle de SHBrowseForFolder, lpstrTitle) doit
# apparaître dans la boîte de fichiers ; le comportement est testé sous Xvfb.
ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok((SCRIPTS / "test-pick-consigne.py").is_file() and "scripts/test-pick-consigne.py" in ci,
   "boîte de fichiers : test Xvfb de la consigne du programme présent et lancé par la CI (#2400)")
