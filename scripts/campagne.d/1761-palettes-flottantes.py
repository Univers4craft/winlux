# #1761 : panneaux flottants de Paint.NET (fenêtres outil) sans Réduire ni Agrandir.
user32 = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
x11 = (ROOT / "runtime/src/dlls/gui_x11.rs").read_text(encoding="utf-8")
# Noms : captioned_tool_window, set_tool_type et le test palette_flottante_reconnue (#2711).
ok("fn captioned_tool_window" in user32 and "set_tool_type" in user32 and "fn palette_flottante_reconnue" in user32,
   "fenêtre outil avec barre de titre reconnue à la création (#1761)")
ok('b"_NET_WM_WINDOW_TYPE_TOOLBAR"' in x11, "fenêtre outil typée barre d'outils pour openbox, bouton Fermer seul (#1761)")
