# #2852 : dans Enregistrer sous, le nom proposé doit arriver sélectionné, Ctrl+A le
# sélectionner et la frappe le remplacer ; le comportement est testé sous Xvfb.
ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok((SCRIPTS / "test-pick-selection.py").is_file() and "scripts/test-pick-selection.py" in ci,
   "boîte de fichiers : test Xvfb du nom sélectionné et remplacé présent et lancé par la CI (#2852)")
