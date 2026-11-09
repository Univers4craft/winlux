applications = (SCRIPTS / "winlux-applications.py").read_text(encoding="utf-8")
ok("def fix_case(path):" in applications and "return fix_case(_windows_path(value, home))" in applications,
   "chemin Windows d'un raccourci résolu sans tenir compte de la casse (#1285)")
ok((SCRIPTS / "test-lnk-casse.py").is_file(), "test du raccourci Audacity.lnk vers audacity.exe (#1285)")
