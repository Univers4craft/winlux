# #1680 : fonctions gdi32/user32 importées par iCUE 4.18.
u = (runtime_dlls / "user32_releve.rs").read_text(encoding="utf-8")
w = (runtime_dlls.parent / "wow32" / "mod.rs").read_text(encoding="utf-8")
ok('"ScaleViewportExtEx" => fn ' in u, "ScaleViewportExtEx servie (#1680)")
ok('"GetMenuCheckMarkDimensions" => fn ' in u, "GetMenuCheckMarkDimensions servie (#1680)")
ok('"GrayStringW" => fn ' in u and '"GrayStringW" => bit(2)' in w,
   "GrayStringW servie, fonction de sortie x86 par passerelle (#1680)")
ok('"RealChildWindowFromPoint" => fn ' in u, "RealChildWindowFromPoint servie (#1680)")
