# #1754 : FindWindow(Ex)A/W reconnaît un atome de classe (MAKEINTATOM).
u = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
# Noms : read_class_name_a/w et le test atome_de_classe_relu_comme_nom (#2711).
ok("fn read_class_name_a(" in u and "fn read_class_name_w(" in u and "fn atome_de_classe_relu_comme_nom" in u,
   "FindWindow lit un atome de classe sans le déréférencer (#1754)")
