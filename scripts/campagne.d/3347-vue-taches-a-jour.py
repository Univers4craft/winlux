# #3347 : la vue des tâches suit les fenêtres qui s'ouvrent ou se ferment
# pendant qu'elle est affichée, en écoutant _NET_CLIENT_LIST sur la racine.
# Il faut un serveur X et openbox : c'est vérifié sous Xvfb par
# scripts/test-taskview-a-jour.py, lancé par l'étape test-all.d nommée ici.
_etape_3347 = (SCRIPTS / "test-all.d/3347-vue-taches-a-jour.sh").read_text(encoding="utf-8")
_test_3347 = (SCRIPTS / "test-taskview-a-jour.py").read_text(encoding="utf-8")
ok("scripts/test-taskview-a-jour.py" in _etape_3347
   and "la carte d'une fenêtre fermée disparaît de la vue ouverte (#3347)" in _test_3347
   and "les fenêtres ouvertes pendant la vue y ont leur carte (#3347)" in _test_3347,
   "vue des tâches : fenêtres ouvertes ou fermées pendant la vue, vérifié sous Xvfb (#3347)")
