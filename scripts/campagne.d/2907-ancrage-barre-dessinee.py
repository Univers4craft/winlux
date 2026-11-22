# #2907 : une fenêtre qui dessine sa barre de titre (Explorateur,
# _NET_WM_MOVERESIZE) s'ancre et se décroche à la souris ; vérifié sous
# openbox réel par test-snap-mouse.py (fenêtre « ancrage-dessinee »).
_s_2907 = (SCRIPTS / "test-snap-mouse.py").read_text(encoding="utf-8")
ok("ancrage-dessinee" in _s_2907,
   "Ancrage à la souris d'une fenêtre à barre de titre dessinée (#2907)")
