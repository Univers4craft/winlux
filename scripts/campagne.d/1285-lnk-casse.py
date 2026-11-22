applications = (SCRIPTS / "winlux-applications.py").read_text(encoding="utf-8")
ok("fix_case(" in applications and "fix_case(" in applications,
   "chemin Windows d'un raccourci résolu sans tenir compte de la casse (#1285)")
ok((SCRIPTS / "test-lnk-casse.py").is_file(), "test du raccourci Audacity.lnk vers audacity.exe (#1285)")
