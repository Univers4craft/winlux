# #1331 : CreateCommandList rend une ID2D1CommandList utilisable comme cible et comme image.
ok("dc_create_command_list as *const ()" in (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
   and 'unsupported!("CreateCommandList")' not in (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8"),
   "Liste de commandes Direct2D (#1331)")
