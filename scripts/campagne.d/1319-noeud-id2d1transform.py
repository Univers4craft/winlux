# #1319 : MapInputRectsToOutputRect est appelée sur l'interface ID2D1Transform du nœud.
ok("com_qi(node, &IID_TRANSFORM)" in (runtime_dlls / "d2d1_effects.rs").read_text(encoding="utf-8")
   and ">(transform, 5)" in (runtime_dlls / "d2d1_effects.rs").read_text(encoding="utf-8"),
   "Nœud de transformation par ID2D1Transform (#1319)")
