# #3066 : la vue des tâches (Windows + Tab) range les fenêtres en grille et
# garde la bande des bureaux dans l'écran ; avant, un FlowBox centré les
# empilait en une colonne et la bande sortait de l'écran dès trois fenêtres.
# Vérifié sous Xvfb avec openbox par scripts/test-taskview-grille.py.
_vue_3066 = runpy.run_path(str(SCRIPTS / "winlux-taskview.py"), run_name="campagne_3066")
_tient_3066 = []
for (_l, _h) in ((1366, 768), (1280, 800), (1920, 1080), (1024, 600)):
    for _n in (1, 3, 6, 12, 20, 40):
        _c, _w, _t = _vue_3066["grid_layout"](_n, _l, _h)
        _rangs = -(-_n // _c)
        _largeur = _c * (_w + _vue_3066["CARD_W"]) + (_c - 1) * _vue_3066["GAP"] + 2 * _vue_3066["MARGIN"]
        _hauteur = _rangs * (_t + _vue_3066["CARD_H"]) + (_rangs - 1) * _vue_3066["GAP"] + 2 * _vue_3066["MARGIN"] + _vue_3066["STRIP"]
        _tient_3066.append(_largeur <= _l and _hauteur <= _h)
print(f"  grille 3 fenêtres en 1366x768 : {_vue_3066['grid_layout'](3, 1366, 768)}")
ok(all(_tient_3066) and _vue_3066["grid_layout"](3, 1366, 768)[0] == 3
   and (SCRIPTS / "test-taskview-grille.py").exists(),
   "vue des tâches : grille et bande des bureaux dans l'écran, jusqu'à 40 fenêtres (#3066)")
