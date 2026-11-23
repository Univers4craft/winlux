# Cadre maison d'un programme 32 bits (#3353) : SetWindowPos(SWP_FRAMECHANGED)
# envoie WM_NCCALCSIZE à la procédure x86 (NCCALCSIZE_PARAMS traduit par
# wow32::user), et cadre32.exe le vérifie.
_user32 = (ROOT / "runtime" / "src" / "dlls" / "user32.rs").read_text(encoding="utf-8")
_debut = _user32.index("unsafe fn frame_changed(")
_garde = _user32[_debut:_user32.index("let mut pos = WindowPos", _debut)]
_etape = SCRIPTS / "test-all.d" / "3353-cadre-maison-x86.sh"
ok("pointer_size" not in _garde
   and (ROOT / "testc" / "wow32" / "cadre32.c").is_file()
   and _etape.is_file() and "test-cadre-maison32.sh" in _etape.read_text(encoding="utf-8"),
   "SWP_FRAMECHANGED envoie WM_NCCALCSIZE à un programme x86, essayé par cadre32.exe (#3353)")
