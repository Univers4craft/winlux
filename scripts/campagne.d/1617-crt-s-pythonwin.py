# #1617 : fonctions « _s » d'ucrtbase voisines de wmemcpy_s (Pythonwin.exe).
crt = (runtime_dlls / "crt_releve.rs").read_text(encoding="utf-8")
ok(all(f'"{n}" => fn' in crt for n in ("wmemcpy_s", "wmemmove_s", "_strset_s", "_wcsset_s", "_strnset_s",
   "_wcsnset_s", "_wcsupr_s", "_strlwr_s", "_mbscpy_s", "_ltoa_s", "_ultoa_s", "_splitpath_s", "_makepath_s")),
   "wmemcpy_s et les fonctions « _s » voisines servies par ucrtbase (#1617)")
