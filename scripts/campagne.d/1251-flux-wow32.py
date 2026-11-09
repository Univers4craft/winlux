# WINLUX_WOW32_HISTORY=flux[:motif] : passages x86 écrits au fil de l'eau,
# et diagnostic WINDOW_DIAG du dessin pendant WM_DRAWITEM (#1251, #1250).
ok("fn history_stream" in (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8")
   and "fn watch_draws" in (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
   and "WINLUX_WOW32_HISTORY=flux" in (ROOT / "docs/BUILD.md").read_text(encoding="utf-8"),
   "Diagnostics x86 : flux des passages et dessin pendant WM_DRAWITEM (#1251)")
