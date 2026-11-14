# Boîte de dialogue : la procédure posée par SetWindowLongPtr(DWLP_DLGPROC) reçoit les messages (#2421).
_d = (ROOT / "runtime/src/dlls/dialogs.rs").read_text()
_t = (ROOT / "runtime/src/dlls/dialogs/tests.rs").read_text()
ok("const DWLP_DLGPROC" in _d and "fn procedure_remplacee_par_dwlp_dlgproc" in _t,
   "la procédure de dialogue remplacée par DWLP_DLGPROC est appelée (#2421)")
