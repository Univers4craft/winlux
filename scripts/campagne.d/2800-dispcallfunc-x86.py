# #2800 : DispCallFunc appelé par un programme x86 empile les arguments
# en cases de 4 octets (VARIANT de 16 octets, valeur à +8) et appelle le
# code x86, au lieu de rendre E_NOTIMPL (puits d'événements ATL de
# l'installeur Creative Cloud). Comportement gardé par le test
# wow32::dispcall::tests::pile_x86_des_arguments_de_dispcallfunc_2800.
dc_2800 = (ROOT / "runtime" / "src" / "wow32" / "dispcall.rs").read_text(encoding="utf-8")
ok("fn pile_x86_des_arguments_de_dispcallfunc_2800" in dc_2800,
   "DispCallFunc servi aux programmes x86 (#2800)")
