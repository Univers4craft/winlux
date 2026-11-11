# SetWindowPos envoie WM_WINDOWPOSCHANGING puis WM_WINDOWPOSCHANGED : la VCL
# y relit ses bornes, le champ du port de WinSCP n'est plus en (0,0) (#1251).
ok("WM_WINDOWPOSCHANGED, envoyé une fois la fenêtre placée" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and "fn placement_annonce_a_la_fenetre" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and "vcl_bounds()" in (ROOT / "testc/wow32/dlg32.c").read_text(encoding="utf-8"),
   "SetWindowPos annonce le placement à la fenêtre (#1251)")
