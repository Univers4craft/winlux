# CommandLineToArgvW rend des pointeurs de la taille du programme : 4 octets en x86, sinon SteamService.exe ne voit pas « /install » (#1899).
_s = (ROOT / "runtime/src/dlls/shell32.rs").read_text(encoding="utf-8")
ok("pack_argv(&args, crate::wow32::pointer_size())" in _s
   and "size_of::<*mut u16>()" not in _s
   and "fn argv_x86_en_pointeurs_de_4_octets" in _s,
   "argv de CommandLineToArgvW en cases de 4 octets pour un programme x86, avec test (#1899)")
