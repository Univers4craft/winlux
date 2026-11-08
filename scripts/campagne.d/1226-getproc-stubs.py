# Diagnostic WINLUX_GETPROC_STUBS=1 : bouchons journalisés pour GetProcAddress,
# désactivé par défaut (installeur Creative Cloud, #1226).
ok("WINLUX_GETPROC_STUBS" in (ROOT / "runtime/src/thunks.rs").read_text(encoding="utf-8")
   and "fn getproc_stub" in (ROOT / "runtime/src/thunks.rs").read_text(encoding="utf-8")
   and "WINLUX_GETPROC_STUBS=1 rend un bouchon" in (SCRIPTS / "test-wow32.sh").read_text(encoding="utf-8")
   and "WINLUX_GETPROC_STUBS" in (ROOT / "docs/BUILD.md").read_text(encoding="utf-8"),
   "GetProcAddress : bouchons de diagnostic seulement avec WINLUX_GETPROC_STUBS=1 (#1226)")
