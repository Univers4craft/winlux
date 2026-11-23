# KernelBase est une vraie image PE avec sa table d'exports, comme les autres
# DLL intégrées : plus aucune page « MZ » seule (fake_module) qu'un programme
# lisant la table (bac à sable Chromium, PEImage) trouverait vide (#3337).
mod_3337 = (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8")
pe_3337 = (ROOT / "runtime/src/dlls/pe_image.rs").read_text(encoding="utf-8")
ok('pe_image::build("kernelbase"' in mod_3337
   and "fake_module" not in mod_3337
   and "fn kernelbase_image_pe_avec_sa_table_3337" in pe_3337
   and "fn aucune_dll_integree_sans_en_tete_pe_3337" in pe_3337,
   "KernelBase : image PE avec table d'exports (#3337)")
