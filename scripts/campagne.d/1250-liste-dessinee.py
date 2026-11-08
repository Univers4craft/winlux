# Liste déroulante dessinée par le programme (superclasse de COMBOBOX,
# TComboBox csOwnerDrawFixed de WinSCP) vérifiée en x86, et WINLUX_WINDOW_DIAG
# qui journalise les messages CB_* et WM_DRAWITEM (#1250).
ok("static void owner_combo" in (ROOT / "testc/wow32/dlg32.c").read_text(encoding="utf-8")
   and "winlux[fenetre] WM_DRAWITEM" in (ROOT / "runtime/src/dlls/controls.rs").read_text(encoding="utf-8"),
   "Liste dessinée par le programme en x86 et diagnostic WINLUX_WINDOW_DIAG (#1250)")
