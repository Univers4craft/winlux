# PEB 32 bits complet (Ldr, ProcessParameters, LoaderLock) : la CRT
# Microsoft liée en statique le lit à la sortie de pyi32.exe et Steam.exe (#1440).
ok("fill_peb32_tables" in (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8")
   and "(#1440)" in (ROOT / "testc/wow32/hello32.c").read_text(encoding="utf-8"),
   "PEB 32 bits rempli pour la sortie des programmes x86 (#1440)")
