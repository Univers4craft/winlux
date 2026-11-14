# Écran verrouillé : la carte « Ces applications empêchent l'arrêt » n'offre
# que « Annuler » et « Déverrouiller » ; forcer l'arrêt exige le mot de passe
# (#2300, décision de damien du 14 nov., différent de Windows). Le
# comportement est vérifié sous Xvfb par scripts/test-power-carte-verrou.py.
ok((ROOT / "scripts/test-power-carte-verrou.py").exists()
   and "test-power-carte-verrou.py" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
   and "test-power-carte-verrou.py" in (ROOT / "scripts/test-all.d/2300-carte-arret-verrou.sh").read_text(encoding="utf-8"),
   "test Xvfb de la carte d'arrêt sur le verrou, lancé par la CI et test-all (#2300)")
