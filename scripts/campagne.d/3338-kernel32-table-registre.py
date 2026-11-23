# La table d'exports de l'image kernel32 porte aussi les noms que
# GetProcAddress(kernel32, …) fait suivre vers advapi32 (Reg*) et ntdll
# (RtlFillMemory) : un programme qui lit la table les trouve (#3338).
mod_3338 = (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8")
pe_3338 = (ROOT / "runtime/src/dlls/pe_image.rs").read_text(encoding="utf-8")
ok("fn kernel32_image_exports(" in mod_3338
   and "kernel32_image_exports(all, &b.exports)" in mod_3338
   and "fn kernel32_table_avec_registre_et_rtl_3338" in pe_3338,
   "kernel32 : Reg* et RtlFillMemory dans la table d'exports (#3338)")
