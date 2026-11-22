# #2909 : une fenêtre ancrée décrochée près du bas garde sa barre de titre
# sous la souris (hauteur réduite au-dessus de la barre des tâches) ; vérifié
# sous openbox réel par test-snap-mouse.py.
_t_2909 = (SCRIPTS / "test-snap-mouse.py").read_text(encoding="utf-8")
ok("décrochée près du bas : barre de titre sous la souris" in _t_2909,
   "Ancrage : décrochée près du bas, la barre de titre reste sous la souris (#2909)")
