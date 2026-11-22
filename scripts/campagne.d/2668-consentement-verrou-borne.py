# Le test des fenêtres de consentement retente, borné, les verrous qui
# doivent se prendre : un enfant entre fork et exec les tient encore (#2668).
# Ce contrôle vérifie le nom du test qui porte ce comportement.
_s = (ROOT / "runtime/src/dlls/saisie.rs").read_text(encoding="utf-8")
ok("fn verrou_des_fenetres_de_consentement(" in _s,
   "test des fenêtres de consentement : verrous retentés (#2668)")
