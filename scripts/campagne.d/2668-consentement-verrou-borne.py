# Le test des fenêtres de consentement retente, borné, les verrous qui
# doivent se prendre : un enfant entre fork et exec les tient encore (#2668).
_s = (ROOT / "runtime/src/dlls/saisie.rs").read_text()
_t = _s[_s.index("fn verrou_des_fenetres_de_consentement"):]
ok("assert_eq!(exclusif(&gate), 0)" in _t and "assert_eq!(exclusif(&window), 0)" in _t,
   "test des fenêtres de consentement : verrous retentés (#2668)")
