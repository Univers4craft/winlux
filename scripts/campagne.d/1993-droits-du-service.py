# SetServiceObjectSecurity accepte un appelant élevé et QueryServiceObjectSecurity relit le descripteur (#1993).
_s = (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/dlls/security/tests.rs").read_text(encoding="utf-8")
ok("if !crate::dlls::security::elevated()" in _s
   and "lock(service_security()).insert(key, stored);" in _s
   and "fn descripteur_absolu_recopie_en_auto_relatif" in _t,
   "SetServiceObjectSecurity : descripteur retenu pour un appelant élevé, relu ensuite (#1993)")
