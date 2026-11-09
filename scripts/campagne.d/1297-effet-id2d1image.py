# #1297 : l'effet Direct2D rend sa sortie pour ID2D1Image (Paint.NET).
ok("*riid == IID_D2D1_IMAGE" in (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
   and "fn refusal_worth_logging" in (runtime_dlls / "comobj.rs").read_text(encoding="utf-8")
   and "refus_d_interface_notes_avec_parcimonie" in (SCRIPTS / "test-all.d" / "1297-effet-id2d1image.sh").read_text(encoding="utf-8"),
   "Effet Direct2D en ID2D1Image (#1297)")
