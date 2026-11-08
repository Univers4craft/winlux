# #1190 : Update.exe de Squirrel (mise à jour de Claude pour Windows) servi
# par winlux-squirrel-update au lieu du CLR, avec confirmation visible.
squirrel_py = (SCRIPTS / "winlux-squirrel-update.py").read_text(encoding="utf-8")
ok("fn is_squirrel_update" in (runtime_dlls.parent / "managed.rs").read_text(encoding="utf-8")
   and "managed::exec_squirrel_update" in (runtime_dlls.parent / "main.rs").read_text(encoding="utf-8")
   and "update_exe_de_squirrel_est_reconnu" in (runtime_dlls.parent / "managed.rs").read_text(encoding="utf-8"),
   "Update.exe de Squirrel reconnu par le runtime et confié à winlux-squirrel-update (#1190)")
ok("def confirm(" in squirrel_py and all(lang in squirrel_py for lang in ('"fr"', '"en"', '"de"', '"es"'))
   and 'scheme not in ("https", "file")' in squirrel_py and "base not in target.parents" in squirrel_py
   and "winlux-squirrel-update" in (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
   and "test-squirrel-update.py" in (SCRIPTS / "test-all.d" / "1190-mise-a-jour-squirrel.sh").read_text(encoding="utf-8"),
   "winlux-squirrel-update : confirmation en 4 langues, HTTPS seul, extraction protégée, dans l'ISO (#1190)")
