# CRT : dernières fonctions sécurisées *_s (_mbs*_s, _gcvt_s, _fcvt_s, _searchenv_s, tmpfile_s) (#2128).
_c = (ROOT / "runtime/src/dlls/crt_secure_reste.rs").read_text()
for _n in ("_mbscat_s", "_mbsncpy_s", "_mbsnbcpy_s", "_mbsncat_s", "_mbsnbcat_s", "_mbstok_s",
           "_mbslwr_s", "_mbsupr_s", "_mbsset_s", "_mbsnset_s", "_mbsnbset_s", "_mbccpy_s",
           "_gcvt_s", "_fcvt_s", "_searchenv_s", "_wsearchenv_s", "tmpfile_s"):
    ok(f'"{_n}" => fn ' in _c, f"CRT exporte {_n} (#2128)")
ok("crt_secure_reste::exports()" in (ROOT / "runtime/src/dlls/mod.rs").read_text(),
   "msvcrt, msvcr80…120 et ucrtbase servent ces fonctions (#2128)")
ok("fn invoke_invalid_parameter()" in (ROOT / "runtime/src/dlls/crt_extra.rs").read_text(),
   "_set_invalid_parameter_handler : le gestionnaire est appelé (#2128)")
ok("fn gcvt_s_exemples_documentes()" in (ROOT / "runtime/src/dlls/crt_secure_reste/tests.rs").read_text(),
   "tests : sorties de l'exemple documenté de _gcvt (#2128)")
