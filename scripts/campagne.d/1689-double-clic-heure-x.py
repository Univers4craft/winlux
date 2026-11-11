# #1689 : double-clic mesuré à l'heure du serveur X ; l'Explorateur prend WM_LBUTTONDBLCLK.
x = (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8")
g = (runtime_dlls.parents[2] / "testexe" / "src" / "bin" / "gui_demo.rs").read_text(encoding="utf-8")
ok("fn is_double_click(" in x and "let now = e.time;" in x,
   "WM_LBUTTONDBLCLK : écart mesuré à l'heure de l'événement X (#1689)")
ok("style: CS_DBLCLKS" in g and "wm::Mouse::DoubleClick" in g,
   "Explorateur : double-clic reçu en WM_LBUTTONDBLCLK (#1689)")
