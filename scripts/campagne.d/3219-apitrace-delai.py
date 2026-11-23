# WINLUX_APITRACE_DELAY : la trace d'API ne commence qu'après le délai donné (#3219).
# Le comportement est vérifié par scripts/test-apitrace-delai.sh (horodatage de la première ligne).
_t = (ROOT / "runtime/src/thunks.rs").read_text(encoding="utf-8")
ok("WINLUX_APITRACE_DELAY" in _t and "fn api_trace_waiting" in _t
   and (ROOT / "scripts/test-apitrace-delai.sh").is_file()
   and "WINLUX_APITRACE_DELAY" in (ROOT / "docs/BUILD.md").read_text(encoding="utf-8"),
   "trace d'API différée par WINLUX_APITRACE_DELAY, testée et documentée (#3219)")
