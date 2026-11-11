# msvcr110/120 : toute la famille __crt* (gldriverquery64.exe de Steam, #1972).
_c = (ROOT / "runtime/src/dlls/crt_releve.rs").read_text()
for _n in ("__crtSetUnhandledExceptionFilter", "__crtFlsAlloc", "__crtInitializeCriticalSectionEx",
           "__crtLCMapStringEx", "__crtGetLocaleInfoEx", "__crtIsPackagedApp", "__crtTerminateProcess"):
    ok(f'"{_n}" => fn crt_' in _c, f"msvcr120 exporte {_n} (#1972)")
ok("fn famille_crt_enveloppe_des_fonctions_existantes()" in _c,
   "test : chaque enveloppe __crt* trouve sa fonction dans kernel32 (#1972)")
