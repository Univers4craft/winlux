# Le bit WS_VISIBLE du style suit la visibilité : ShowWindow et SetWindowPos
# le posent et l'ôtent, SetWindowLongPtr(GWL_STYLE) qui le change montre ou
# cache la fenêtre (#3247).
dlg_tests_3247 = (ROOT / "runtime/src/dlls/dialogs/tests.rs").read_text(encoding="utf-8")
ok("fn ws_visible_du_style_suit_la_visibilite_3247" in dlg_tests_3247,
   "user32 : WS_VISIBLE du style et IsWindowVisible restent d'accord (#3247)")
