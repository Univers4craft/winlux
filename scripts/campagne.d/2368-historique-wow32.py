# WINLUX_WOW32_HISTORY écrit l'historique à la fin du processus, au niveau
# Warn (#2368). Le comportement est vérifié par le test unitaire nommé
# ci-dessous, qui relance l'exécutable ; ce contrôle vérifie les noms.
_m = (ROOT / "runtime/src/wow32/mod.rs").read_text()
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("fn historique_ecrit_a_la_fin_du_processus()" in _m,
   "test : historique écrit sur ExitProcess et fail-fast (#2368)")
ok("pub fn dump_history_at_exit(" in _m and "dump_history_at_exit(" in _k,
   "historique wow32 écrit à la fin du processus (#2368)")
