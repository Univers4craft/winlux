# #2531 : GDI+ lit et écrit l'IStream x86 d'un programme 32 bits par un
# mandataire (l'outil de déploiement d'Office fautait dans
# GdipLoadImageFromStream).
c = (ROOT / "runtime/src/wow32/com.rs").read_text(encoding="utf-8")
t = (ROOT / "testc/wow32/com32.c").read_text(encoding="utf-8")
ok("fn flux_gdiplus_decrits" in c and "#2531" in t,
   "gdiplus : IStream x86 lu et écrit par un mandataire (#2531)")
