# #996 : diagnostic WINLUX_COMTRACE sur les tables DXGI et Direct3D 11.
ok('"WINLUX_COMTRACE"' in (ROOT / "runtime/src/thunks.rs").read_text(encoding="utf-8")
   and "com_trace_table(t, module, label)" in (runtime_dlls / "dx11_x86.rs").read_text(encoding="utf-8")
   and "trace_com_rend_les_valeurs_et_journalise" in (SCRIPTS / "test-all.d" / "0996-trace-com.sh").read_text(encoding="utf-8"),
   "WINLUX_COMTRACE trace les méthodes DXGI et Direct3D 11 (#996)")
