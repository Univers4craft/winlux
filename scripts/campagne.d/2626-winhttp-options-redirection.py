# WinHttpSetOption accepte DISABLE_FEATURE (63), AUTOLOGON_POLICY (77) et
# REDIRECT_POLICY (88), et les redirections en tiennent compte (#2626).
_s = (ROOT / "runtime/src/dlls/winhttp.rs").read_text()
ok("fn options_redirection_et_ouverture_de_session_2626" in _s,
   "test des options 63, 77 et 88 de WinHttpSetOption (#2626)")
ok("fn redirections_suivies_ou_rendues_2626" in _s,
   "test des redirections suivies ou rendues (#2626)")
