# #1690 : la boîte Ouvrir lit une longue liste de types (IrfanView).
p = (runtime_dlls.parents[2] / "testexe" / "src" / "shell" / "explorer_pick.rs").read_text(encoding="utf-8")
ok("static mut ENV_W: [u16; 32768]" in p and "const PAT_LEN: usize = 2048;" in p,
   "Ouvrir : liste des types lue jusqu'à 32 767 caractères (#1690)")
ok("const LIST_ROWS: usize = 12;" in p and "fn keep_visible()" in p,
   "Ouvrir : liste des types qui défile (#1690)")
