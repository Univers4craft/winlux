# Combo dessinée par son propriétaire : WM_MEASUREITEM à la création et hauteur du champ dès CreateWindowEx (#3222).
_t = (ROOT / "runtime/src/dlls/controls/tests.rs").read_text()
ok("fn combo_dessinee_par_le_proprietaire_mesuree_a_la_creation" in _t,
   "combo dessinée par son propriétaire mesurée à la création (#3222)")
