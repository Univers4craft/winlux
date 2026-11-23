# La CRT que ntdll exporte sous Windows (mingw-w64 ntdllcrt.def.in) et que
# notre CRT sert est dans la table d'exports de l'image ntdll, pas seulement
# dans le repli vers msvcrt (Acrobat, i21 : swprintf_s, #3389).
nt_3389 = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
mod_3389 = (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8")
ok("pub const CRT_REEXPORTS" in nt_3389
   and '"swprintf_s",' in nt_3389
   and "fn crt_de_ntdll_dans_l_image_3389" in nt_3389
   and "add_ntdll_crt(&mut all);" in mod_3389,
   "ntdll : CRT réexportée dans la table d'exports de l'image (#3389)")
