# Programme 32 bits agrandi par le gestionnaire (#3394) : la marque
# BACKEND_GEOMETRY survit au MSG x86, qui n'a pas de lPrivate, et
# geometry_changed envoie WM_WINDOWPOSCHANGED à la procédure x86.
_user32 = (ROOT / "runtime" / "src" / "dlls" / "user32.rs").read_text(encoding="utf-8")
_debut = _user32.index("unsafe fn geometry_changed(")
_garde = _user32[_debut:_user32.index("let Some(rect)", _debut)]
_wow = (ROOT / "runtime" / "src" / "wow32" / "user.rs").read_text(encoding="utf-8")
_etape = SCRIPTS / "test-all.d" / "3394-agrandie-x86.sh"
ok("pointer_size" not in _garde and "fn retenir_marque(" in _wow
   and (ROOT / "testc" / "wow32" / "agrandie32.c").is_file()
   and _etape.is_file() and "test-agrandie32.sh" in _etape.read_text(encoding="utf-8"),
   "programme x86 agrandi : WM_WINDOWPOSCHANGED avant WM_SIZE, essayé par agrandie32.exe (#3394)")
