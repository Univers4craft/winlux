# #2499 : le puits de géométrie reçu n'est jamais fermé par nous.
d = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
i = d.index("unsafe fn emit_figures(")
corps = d[i:d.index("\n}\n", i)]
ok("Close" not in corps,
   "Simplify, Widen, Combine et Stream laissent le puits ouvert (#2499)")
