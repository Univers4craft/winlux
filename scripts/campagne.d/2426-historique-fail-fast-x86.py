# WINLUX_WOW32_HISTORY : un fail-fast exécuté par le code x86 (int 0x29)
# passe par le gestionnaire de plantage, qui écrit l'historique de tous les
# fils sans allocation ni verrou (#2426). Le comportement est vérifié par le
# test unitaire nommé ci-dessous (vrai int 0x29 dans un processus relancé) ;
# ce contrôle vérifie les noms.
_m = (ROOT / "runtime/src/wow32/mod.rs").read_text()
_c = (ROOT / "runtime/src/crash.rs").read_text()
ok("pub(crate) unsafe fn dump_history_in_signal(" in _m
   and "dump_history_in_signal(" in _c,
   "historique wow32 écrit par le gestionnaire de plantage (#2426)")
ok('"int29"' in _m and "fn historique_ecrit_a_la_fin_du_processus()" in _m,
   "test : historique écrit sur un int 0x29 réel (#2426)")
