# #1618 : FindFirstFile sur un dossier vide rend « . » et « .. » (pip, shutil.rmtree).
wf = (runtime_dlls / "winfile.rs").read_text(encoding="utf-8")
ok("fn is_drive_root" in wf and "dossier_vide_rend_point_et_point_point" in wf,
   "FindFirstFile rend « . » et « .. » hors racine de lecteur, même pour un dossier vide (#1618)")
