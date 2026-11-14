# #2431 : WINLUX_D2D_TRACE détaille chaque trait (figures, motif, boîte).
d = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("unsafe fn trace_stroke(" in d and "morceaux, {} polygones boîte" in d,
   "le journal D2D détaille les traits (figures, motif, boîte) (#2431)")
