# #1479 : __dllonexit remplit la table de la DLL (7z.dll), pas la liste du processus.
crt = (runtime_dlls / "crt_runtime.rs").read_text(encoding="utf-8")
ok("pub unsafe extern \"win64\" fn dllonexit(f: usize, begin: *mut usize, end: *mut usize)" in crt,
   "CRT : __dllonexit range la fonction dans la table de la DLL (#1479)")
