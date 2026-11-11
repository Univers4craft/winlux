# #1809 : le lanceur CEF de Spotify charge Spotify.dll par LoadLibraryEx(DONT_RESOLVE_DLL_REFERENCES).
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("if flags & 0x1 != 0 {\n        return load_as_data(&value, 0);" in k,
   "LoadLibraryEx DONT_RESOLVE_DLL_REFERENCES projette l'image sans imports ni DllMain (#1809)")
ok("fn load_library_ex_sans_resoudre_les_references" in k,
   "test : GetModuleFileName, FindResource et FreeLibrary sur l'image non résolue (#1809)")
