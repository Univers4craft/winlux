# #1354 : un contexte de couleur sRGB rend un profil ICC complet.
ok("pub fn srgb" in (runtime_dlls / "icc.rs").read_text(encoding="utf-8")
   and "crate::dlls::icc::srgb()" in (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8"),
   "Profil ICC sRGB du contexte de couleur Direct2D (#1354)")
