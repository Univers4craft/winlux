# Trace des fils : handle, fil désigné et valeur rendue dans le journal d'API (#2161).
_f = (ROOT / "runtime/src/dlls/fil_trace.rs").read_text()
ok("winlux[fil]" in _f and "fn active()" in _f and "api_trace_active" in _f,
   "Trace des fils : lignes winlux[fil] seulement avec WINLUX_APITRACE (#2161)")
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
for _n in ("WaitForSingleObject", "GetExitCodeThread", "ResumeThread", "CreateThread", "CloseHandle"):
    ok(f'"{_n}"=>fn super::fil_trace::' in _k, f"Trace des fils : {_n} passe par fil_trace (#2161)")
_t = (ROOT / "runtime/src/thunks.rs").read_text()
ok("pub fn api_clock()" in _t and "@{:.6}" in _t, "Journal d'API horodaté (#2161)")
