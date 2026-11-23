# #3346 : une fenêtre affichée sur tous les bureaux (bureau -1 pour wmctrl)
# a sa carte dans la vue des tâches de chaque bureau, à côté des fenêtres
# du bureau. Le tri est vérifié ici sur des listes ; la vue elle-même l'est
# sous Xvfb par scripts/test-taskview-tous-bureaux.py (étape test-all.d 3346).
_vue_3346 = runpy.run_path(str(SCRIPTS / "winlux-taskview.py"), run_name="campagne_3346")
_fen_3346 = [("0x1", 0, "Ici"), ("0x2", -1, "Partout"), ("0x3", 1, "Ailleurs")]
ok([w for w, _, _ in _vue_3346["on_desktop"](_fen_3346, 0)] == ["0x1", "0x2"]
   and [w for w, _, _ in _vue_3346["on_desktop"](_fen_3346, 1)] == ["0x2", "0x3"]
   and [w for w, _, _ in _vue_3346["on_desktop"](_fen_3346, 2)] == ["0x2"],
   "vue des tâches : une fenêtre sur tous les bureaux figure sur chacun (#3346)")
_etape_3346 = (SCRIPTS / "test-all.d/3346-vue-taches-tous-bureaux.sh").read_text(encoding="utf-8")
ok("scripts/test-taskview-tous-bureaux.py" in _etape_3346,
   "vue des tâches : fenêtre sur tous les bureaux vérifiée sous Xvfb (#3346)")
