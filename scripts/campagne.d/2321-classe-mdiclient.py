# #2321 : la classe système MDICLIENT existe ; sans elle, toute application
# MDI (Pythonwin, MFC CMDIFrameWnd) sortait en 0 sans fenêtre.
ctl = (ROOT / "runtime/src/dlls/controls.rs").read_text(encoding="utf-8")
mdi = (ROOT / "runtime/src/dlls/mdi.rs").read_text(encoding="utf-8")
ok('"mdiclient" => super::mdi::mdiclient_proc' in ctl
   and "WM_MDICREATE => create_child" in mdi
   and "fn client_mdi_cree_active_et_detruit_ses_enfants" in mdi,
   "user32 : classe MDICLIENT et enfants MDI (#2321)")
