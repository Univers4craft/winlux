# SetWindowSubclass d'un programme x86 (Notepad++, par numéro d'ordre 410)
# : la SUBCLASSPROC reçoit une passerelle 64 → 32 au lieu d'être appelée
# comme du code win64 (#2876).
_user = (ROOT / "runtime" / "src" / "wow32" / "user.rs").read_text(encoding="utf-8")
_etape = (SCRIPTS / "test-all.d" / "2876-sous-classement-x86.sh")
ok("fn sous_classement_x86_servi_2876" in _user and "fn subclass_dispatch" in _user
   and _etape.is_file() and "test-subclass32.sh" in _etape.read_text(encoding="utf-8")
   and (ROOT / "testc" / "wow32" / "subclass32.c").is_file(),
   "le sous-classement comctl32 d'un programme x86 est servi et essayé par subclass32.exe (#2876)")
