# #3350 : « Déplacer vers › Nouveau bureau » dans la vue des tâches nomme le
# bureau créé « Bureau N », comme Win+Ctrl+D ; sans nom, openbox l'appelait
# « desktop N », en anglais, dans sa bulle de changement de bureau. Il faut
# un serveur X et openbox : c'est vérifié sous Xvfb par
# scripts/test-taskview-nouveau-bureau.py, lancé par l'étape test-all.d nommée ici.
_etape_3350 = (SCRIPTS / "test-all.d/3350-vue-taches-nouveau-bureau-nom.sh").read_text(encoding="utf-8")
_test_3350 = (SCRIPTS / "test-taskview-nouveau-bureau.py").read_text(encoding="utf-8")
ok("scripts/test-taskview-nouveau-bureau.py" in _etape_3350
   and "le bureau créé par « Déplacer vers › Nouveau bureau » s'appelle « Bureau 2 » (#3350)" in _test_3350,
   "vue des tâches : « Déplacer vers › Nouveau bureau » nomme le bureau, vérifié sous Xvfb (#3350)")
