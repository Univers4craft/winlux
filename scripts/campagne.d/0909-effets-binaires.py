# D3DX9 (#909) : effets fx_2_0 déjà compilés lus au lieu d'être refusés.
ok((runtime_dlls / "d3dx9_effect" / "binaire.rs").exists()
   and "format non pris en charge, " not in (runtime_dlls / "d3dx9_effect.rs").read_text()
   and "binaire::read(src)" in (runtime_dlls / "d3dx9_effect.rs").read_text(),
   "D3DX9 : effets fx_2_0 binaires (.fxo) lus (#909)")
