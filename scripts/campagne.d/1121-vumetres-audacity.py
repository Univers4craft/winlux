# Masques monochromes de wxWidgets : une copie couleur vers une bitmap 1 bit
# rend blanc le fond du DC source (graduation des vumètres d'Audacity).
ok("fn to_monochrome" in (runtime_dlls / "gdi32.rs").read_text()
   and "masque_monochrome_tire_de_la_couleur_de_fond" in (SCRIPTS / "test-all.sh").read_text(),
   "Masques monochromes (couleur de fond → blanc) : graduation des vumètres d'Audacity (#1121)")
