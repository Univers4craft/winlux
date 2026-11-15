# #2537 : l'OVERLAPPED d'un programme 32 bits (Offset à +8, hEvent à +16)
# est lu à sa disposition par ReadFile et WriteFile (Google Update).
k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
ok("fn ov_offset" in k and "fn ov_get" in k
   and "fn overlapped_x86_ecriture_positionnee" in k,
   "OVERLAPPED x86 : position et champs à la disposition de l'appelant, testé (#2537)")
