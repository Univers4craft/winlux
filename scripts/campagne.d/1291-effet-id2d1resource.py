# #1291 : l'effet Direct2D rend sa fabrique par ID2D1Resource (Paint.NET).
ok("EFFECT_RESOURCE_VT" in (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
   and "effet_rend_sa_fabrique_par_id2d1resource" in (SCRIPTS / "test-all.d" / "1291-effet-id2d1resource.sh").read_text(encoding="utf-8"),
   "Effet Direct2D : ID2D1Resource et GetFactory (#1291)")
