# Origine logique du DC (SetWindowOrgEx, SetViewportOrgEx…) appliquée au dessin :
# libellés de la VCL de Delphi (boîte Login de WinSCP) (#1232).
ok("fn translate_cmd" in (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
   and "set_origin(hdc, false, x, y, prev)" in (runtime_dlls / "gdi32.rs").read_text(encoding="utf-8")
   and "DrawTextW suit SetWindowOrgEx" in (ROOT / "testc/wow32/dlg32.c").read_text(encoding="utf-8"),
   "Origine de fenêtre et de viewport du DC appliquée au texte, aux formes, aux copies et à la découpe (#1232)")
