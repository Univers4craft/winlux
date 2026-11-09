# #1374 : l'exposition d'une fenêtre Direct3D ne la repeint pas en gris.
ok("crate::dlls::gui::mark_direct3d_window(drawable)" in (runtime_dlls / "dxgi.rs").read_text(encoding="utf-8")
   and "if self.presented_by_direct3d(e.window)" in (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8")
   and "marque_direct3d_lue_par_le_proprietaire_x11" in (SCRIPTS / "test-all.d/1374-exposition-direct3d.sh").read_text(encoding="utf-8"),
   "Fenêtre Direct3D exposée : WM_PAINT, pas de fond GDI (#1374)")
