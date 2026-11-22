# #2893 : l'assistant d'installation garde sa largeur pendant l'installation,
# même quand la ligne d'état est très longue. Le comportement est vérifié sous
# Xvfb par scripts/test-setup-wizard-largeur.py (étape de test-all).
ok((ROOT / "scripts" / "test-setup-wizard-largeur.py").exists()
   and (ROOT / "scripts" / "test-all.d" / "2893-assistant-largeur.sh").exists(),
   "Assistant d'installation : largeur fixe malgré une longue ligne d'état (#2893)")
