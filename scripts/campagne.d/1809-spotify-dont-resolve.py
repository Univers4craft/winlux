# #1809 : le lanceur CEF de Spotify charge Spotify.dll par LoadLibraryEx(DONT_RESOLVE_DLL_REFERENCES).
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
# Noms : load_as_data et le test load_library_ex_sans_resoudre_les_references (#2711).
ok("fn load_as_data(" in k and "fn load_library_ex_sans_resoudre_les_references" in k,
   "LoadLibraryEx DONT_RESOLVE_DLL_REFERENCES projette l'image sans imports ni DllMain (#1809)")
ok("fn load_library_ex_sans_resoudre_les_references" in k,
   "test : GetModuleFileName, FindResource et FreeLibrary sur l'image non résolue (#1809)")
