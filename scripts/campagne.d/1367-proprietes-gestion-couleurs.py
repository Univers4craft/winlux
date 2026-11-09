# #1367 : l'effet Direct2D de gestion des couleurs décrit ses propriétés.
ok("fn builtin_props(clsid: &Guid)" in (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8"),
   "Direct2D : propriétés de l'effet de gestion des couleurs (#1367)")
