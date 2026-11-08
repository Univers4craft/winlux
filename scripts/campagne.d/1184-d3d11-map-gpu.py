# #1184 : Map en écriture d'un tampon écrit par le GPU relit d'abord GL.
ok("fn mark_gpu_written(" in (runtime_dlls / "d3d11.rs").read_text(encoding="utf-8")
   and "#1184" in (ROOT / "testc/msvc/hlsl_msvc.c").read_text(encoding="utf-8")
   and "#1184" in (SCRIPTS / "test-mingw.sh").read_text(encoding="utf-8"),
   "Direct3D 11 : Map WRITE d'un tampon écrit par le GPU ne renvoie plus de copie périmée (#1184)")
