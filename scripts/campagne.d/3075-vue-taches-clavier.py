# #3075 : la vue des tâches se pilote au clavier : flèches, Entrée, Suppr,
# Tab vers la bande des bureaux. Le comportement demande un vrai serveur X
# et openbox : il est vérifié sous Xvfb par scripts/test-taskview-clavier.py,
# lancé par l'étape test-all.d nommée ici.
_etape_3075 = (SCRIPTS / "test-all.d/3075-vue-taches-clavier.sh").read_text(encoding="utf-8")
_test_3075 = (SCRIPTS / "test-taskview-clavier.py").read_text(encoding="utf-8")
ok("scripts/test-taskview-clavier.py" in _etape_3075
   and "Tab puis flèche droite et Entrée mène au bureau suivant (#3075)" in _test_3075
   and "Entrée ouvre la fenêtre choisie et ferme la vue (#3075)" in _test_3075,
   "vue des tâches : flèches, Entrée, Suppr et Tab vérifiés sous Xvfb (#3075)")
