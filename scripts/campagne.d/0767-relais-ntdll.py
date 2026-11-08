# Bac à sable de Chromium (#767) : relais Nt* de ntdll au format Windows, images intégrées à adresse fixe.
ok("fn redirect_service" in (runtime_dlls / "pe_image.rs").read_text()
   and "IMAGES_FIXED" in (runtime_dlls / "pe_image.rs").read_text()
   and "redirect_service" in (runtime_dlls / "seh_hw.rs").read_text()
   and "map_shared_user_data" in (ROOT / "runtime" / "src" / "process.rs").read_text(),
   "ntdll : relais Nt* au format Windows et images intégrées à adresse fixe (#767)")
