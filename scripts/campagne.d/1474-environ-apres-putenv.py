# _environ et _wenviron suivent _putenv : TCL_LIBRARY de PyInstaller visible de Tcl (#1474).
ok("pub fn refresh_environ" in (ROOT / "runtime/src/dlls/crt_runtime.rs").read_text(encoding="utf-8")
   and "(#1474)" in (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8"),
   "environnement CRT à jour après _putenv (#1474)")
