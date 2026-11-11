# #1754 : FindWindow(Ex)A/W reconnaît un atome de classe (MAKEINTATOM).
u = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
ok("Some(read_class_name_a(class))" in u and "Some(read_class_name_w(class))" in u,
   "FindWindow lit un atome de classe sans le déréférencer (#1754)")
