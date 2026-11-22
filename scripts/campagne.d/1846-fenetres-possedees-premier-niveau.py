# Une fenêtre possédée (sans WS_CHILD) est de premier niveau : EnumWindows, WindowFromPoint et GetForegroundWindow la voient (boîte de service Steam, #1846).
_g = (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
_u = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok("fn is_top_level(w: &Window) -> bool" in _g and "w.parent == 0 || w.style & 0x4000_0000 == 0" in _g
   # Nom : top_level_in_z_order, qui trie par is_top_level (#2711).
   and "fn top_level_in_z_order(" in _g,
   "premier niveau = sans parent ou sans WS_CHILD (#1846)")
# Nom du test qui vérifie la boîte énumérée et le bouton absent (#2711).
ok("fn un_clic_active_une_fenetre_surgissante_hors_gestionnaire" in _g,
   "test : boîte possédée énumérée, contrôle non (#1846)")
ok("let active = gui::active_top();" in _u and _u.count("gui::active_top()") >= 2,
   "GetForegroundWindow et GetActiveWindow suivent la fenêtre active (#1846)")
