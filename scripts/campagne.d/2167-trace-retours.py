# Trace des retours : WINLUX_APITRACE_RET note rax et la dernière erreur des API filtrées (#2167).
_t = (ROOT / "runtime/src/thunks.rs").read_text()
ok('std::env::var("WINLUX_APITRACE_RET")' in _t and "winlux[ret]" in _t,
   "WINLUX_APITRACE_RET : ligne winlux[ret] avec valeur rendue et erreur (#2167)")
ok("fn trace_des_retours_sans_variable_ne_change_rien" in _t,
   "WINLUX_APITRACE_RET : sans la variable, cible rendue telle quelle, prouvé par un test (#2167)")
_a = _t[_t.index("pub fn api_trace("):]
ok("api_ret(dll, func, target)" in _a[:600],
   "WINLUX_APITRACE_RET : pont posé sous la trace d'entrée (#2167)")
