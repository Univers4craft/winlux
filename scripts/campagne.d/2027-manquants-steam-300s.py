# Manquants relevés pendant une passe Steam de 300 s (#2027, #2029, #2030, #2031).
_crt = (ROOT / "runtime/src/dlls/crt_releve.rs").read_text()
_sec = (ROOT / "runtime/src/dlls/security.rs").read_text()
_adv = (ROOT / "runtime/src/dlls/advapi32.rs").read_text()
_ord = (ROOT / "runtime/src/dlls/common_apps.rs").read_text()
ok('"__crt_debugger_hook"' in _crt and '"_calloc_crt"' in _crt,
   "msvcr120 : __crt_debugger_hook et _calloc_crt (#2027)")
ok('"GetExplicitEntriesFromAclA" => fn GetExplicitEntriesFromAclW' in _sec,
   "advapi32 : GetExplicitEntriesFromAclA (#2029)")
ok(all(f'"{n}"' in _adv for n in ("RegDisableReflectionKey", "RegEnableReflectionKey", "RegQueryReflectionKey")),
   "advapi32 : réflexion du registre sans effet (#2030)")
ok('("oleaut32", 313, "VarBstrCat")' in _ord, "oleaut32 : ordinal 313 = VarBstrCat (#2031)")
