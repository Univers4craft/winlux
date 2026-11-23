# AdjustWindowRectEx annonce pour un dialogue fixe le cadre sans poignée qu'il reçoit (#3260).
_t = (ROOT / "runtime/src/dlls/user32.rs").read_text()
ok("fn cadre_annonce_d_un_dialogue_fixe_3222" in _t,
   "cadre annoncé d'un dialogue fixe égal à celui qu'il reçoit (#3260)")
