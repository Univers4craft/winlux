# #1416 : Windows.UI.Composition (CompositorController de Paint.NET).
src = (runtime_dlls / "composition.rs").read_text(encoding="utf-8")
ro = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok("Windows.UI.Composition.Core.CompositorController" in src
   and "fn create_desktop_window_target(" in src
   and "fn interop_begin_draw(" in src
   and "composition::activation_factory" in ro,
   "Composition WinRT : contrôleur, cible de fenêtre et surfaces Direct2D (#1416)")
