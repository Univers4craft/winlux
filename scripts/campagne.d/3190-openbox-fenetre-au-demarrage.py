# #3190 : une fenêtre affichée pendant le démarrage d'openbox restait non
# gérée jusqu'au prochain événement X. Le comportement est gardé par
# scripts/test-openbox-demarrage.py (openbox réel sous Xvfb, CI « Tests
# runtime ») ; ici, le correctif publié et les deux tests branchés.
correctif_3190 = ROOT / "integration" / "native" / "openbox" / "0002-fenetre-au-demarrage.patch"
texte_3190 = correctif_3190.read_text(encoding="utf-8") if correctif_3190.is_file() else ""
ok("#3190" in texte_3190 and "+++ b/openbox/openbox.c" in texte_3190,
   "Correctif openbox publié : fenêtre affichée pendant son démarrage gérée sans attendre un autre événement (#3190)")
ci_3190 = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
ok("scripts/test-openbox-demarrage.py" in ci_3190 and (SCRIPTS / "test-openbox-demarrage.py").is_file(),
   "Test sous l'openbox de Winlux : fenêtre affichée dès « wmctrl -m », gérée à chaque tour (#3190)")
perf_3190 = (SCRIPTS / "test-window-performance.py").read_text(encoding="utf-8")
ok("def manager_ready" in perf_3190 and "manager_ready()" in perf_3190,
   "test-window-performance attend la boucle de l'openbox d'Ubuntu avant d'ouvrir les Paramètres (#3190)")
