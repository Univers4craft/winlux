# Un Setup.exe Squirrel sans paquet complet (installeur web de Teams) n'est
# plus refusé par setup : il s'exécute comme tout programme (#3022).
squirrel_src = (ROOT / "runtime/src/setup/squirrel.rs").read_text(encoding="utf-8")
ok("fn installateur_web_sans_paquet_execute_normalement_3022" in squirrel_src,
   "Squirrel : installateur web sans paquet complet exécuté normalement (#3022)")
