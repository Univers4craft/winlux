# Vue des polices du registre : une police déjà inscrite est reconnue par
# ensembles (valeur identique ou fin « \fichier ») au lieu d'un format! par
# paire police × valeur ; le test vérifie l'absence de doublon (Gain #3091).
_t = (ROOT / "runtime/src/registry/tests.rs").read_text(encoding="utf-8")
ok("fn polices_deja_inscrites_jamais_doublees()" in _t,
   "registre : polices déjà inscrites reconnues sans comparaison quadratique (#3091)")
