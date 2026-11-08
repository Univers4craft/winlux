# scripts/test-all.sh lit scripts/test-all.d/*.sh (un fichier par issue), pour
# que deux PR n'ajoutent plus leur étape au même endroit du fichier.
ok('for depot in scripts/test-all.d/*.sh' in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8")
   and (SCRIPTS / "test-all.d" / "0000-dossier-charge.sh").is_file(),
   "scripts/test-all.sh charge scripts/test-all.d (un fichier par étape)")
