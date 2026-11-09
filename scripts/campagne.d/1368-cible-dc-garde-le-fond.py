# #1368 : la cible Direct2D liée à un DC part du contenu du DC.
ok("fn load_dc(&mut self" in (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8"),
   "Direct2D : la cible de DC garde le fond GDI (#1368)")
