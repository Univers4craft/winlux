# Saisie dans un champ et création de fenêtre sur deux fils : pas de verrou mort (#3041).
_t = (ROOT / "runtime/src/dlls/controls/tests.rs").read_text()
ok("fn saisie_et_creation_de_fenetres_sur_deux_fils_sans_verrou_mort" in _t,
   "saisie et création de fenêtres sur deux fils sans verrou mort (#3041)")
