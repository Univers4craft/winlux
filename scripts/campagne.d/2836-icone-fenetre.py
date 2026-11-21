# Fenêtres : l'icône posée par WM_SETICON est publiée dans _NET_WM_ICON par le
# backend X11 ; vérifié sous Xvfb par scripts/test-icone-fenetre.sh (#2836).
_g = (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
_x = (ROOT / "runtime/src/dlls/gui_x11.rs").read_text(encoding="utf-8")
_s = (ROOT / "scripts/test-icone-fenetre.sh").read_text(encoding="utf-8")
ok("fn net_wm_icon(icons: &[usize]) -> Vec<u32>" in _g and "fn set_icon(&mut self, hwnd: usize, icon: &[u32])" in _x
   and "_NET_WM_ICON" in _s and "fn icone_posee_traduite_pour_net_wm_icon" in _g,
   "Fenêtres : icône WM_SETICON publiée dans _NET_WM_ICON, test Xvfb (#2836)")
