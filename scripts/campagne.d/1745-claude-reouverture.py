# #1745 : ShowWindow annonce la visibilité par WM_WINDOWPOSCHANGING/CHANGED
# (SWP_SHOWWINDOW, SWP_HIDEWINDOW), signal suivi par Chromium : Claude
# rouverte depuis la zone de notification ne doit plus rester blanche.
src = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok("unsafe fn visibility_win_pos(hwnd: usize, show: bool)" in src
   and "afficher_et_cacher_annonces_par_windowpos" in src,
   "ShowWindow : SWP_SHOWWINDOW/SWP_HIDEWINDOW annoncés (#1745)")
