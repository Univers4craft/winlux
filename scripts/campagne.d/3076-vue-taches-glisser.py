# #3076 : dans la vue des tâches, une fenêtre glissée sur un bureau de la
# bande y est déplacée ; un clic simple ouvre toujours la fenêtre. Le
# glisser demande un vrai serveur X et openbox : il est vérifié sous Xvfb
# par scripts/test-taskview-glisser.py, lancé par l'étape test-all.d nommée ici.
_etape_3076 = (SCRIPTS / "test-all.d/3076-vue-taches-glisser.sh").read_text(encoding="utf-8")
_test_3076 = (SCRIPTS / "test-taskview-glisser.py").read_text(encoding="utf-8")
ok("scripts/test-taskview-glisser.py" in _etape_3076
   and "carte lâchée sur Bureau 2 : la fenêtre y est déplacée (#3076)" in _test_3076
   and "un clic sur la carte ouvre toujours la fenêtre (#3076)" in _test_3076,
   "vue des tâches : glisser une fenêtre sur un bureau, vérifié sous Xvfb (#3076)")
