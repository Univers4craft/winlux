# #1363 : mscms.dll intégrée (profils de couleur ICM).
ok("\"OpenColorProfileW\"" in (runtime_dlls / "mscms.rs").read_text(encoding="utf-8")
   and "mscms::exports()" in (runtime_dlls / "mod.rs").read_text(encoding="utf-8"),
   "mscms.dll : profils de couleur ICM (#1363)")
