# Fil fautif dans le rapport de faute et trace des fils indépendante de
# WINLUX_APITRACE (#2405). Le comportement est vérifié par les tests
# unitaires nommés ci-dessous, qui relancent l'exécutable ; ce contrôle
# vérifie les noms.
_f = (ROOT / "runtime/src/dlls/fil_trace.rs").read_text()
_c = (ROOT / "runtime/src/crash.rs").read_text()
ok("WINLUX_THREAD_TRACE" in _f and "fn trace_des_fils_sans_trace_d_api()" in _f,
   "test : trace des fils avec WINLUX_THREAD_TRACE seule (#2405)")
ok("fn write_fault_thread()" in _c,
   "rapport de faute : fil fautif nommé (#2405)")
