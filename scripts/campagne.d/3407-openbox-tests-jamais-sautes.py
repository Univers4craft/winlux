# Les tests de l'openbox de Winlux (#3064, #3190) échouent quand un outil
# manque, au lieu de se sauter en laissant l'étape CI verte, et l'étape
# installe wmctrl, que demande le test du démarrage (#3407).
_ci = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
_debut = _ci.index("sous l'openbox de Winlux (#3064)")
_etape = _ci[_debut:_ci.index("- name:", _debut)]
_demarrage = (SCRIPTS / "test-openbox-demarrage.py").read_text(encoding="utf-8")
_boutons = (SCRIPTS / "test-openbox-boutons.py").read_text(encoding="utf-8")
ok("wmctrl" in _etape.split("sh scripts/build-openbox.sh")[0]
   and "test sauté" not in _demarrage and "test sauté" not in _boutons,
   "tests de l'openbox de Winlux : wmctrl installé, aucun outil manquant ne saute le test (#3407)")
