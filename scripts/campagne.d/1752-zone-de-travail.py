# #1752 : fenêtre restaurée sous la barre des tâches (Audacity) ramenée dans la zone de travail.
user32 = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
monitors = (ROOT / "runtime/src/dlls/monitors.rs").read_text(encoding="utf-8")
x11 = (ROOT / "runtime/src/dlls/gui_x11.rs").read_text(encoding="utf-8")
ok("fn fit_to_work_area" in user32 and "fit_to_work_area(hwnd);" in user32,
   "fenêtre à barre de titre ramenée dans la zone de travail à l'affichage (#1752)")
ok("pub fn fit_in_work_area" in monitors, "calcul du rectangle dans la zone de travail, cadre compris (#1752)")
ok('b"_NET_REQUEST_FRAME_EXTENTS"' in x11, "épaisseur du cadre demandée au gestionnaire avant l'affichage (#1752)")
