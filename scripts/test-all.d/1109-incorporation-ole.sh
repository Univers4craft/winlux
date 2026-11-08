# Incorporation OLE (#1109) : GetClassFile, OleCreateFromFile (docfile copié
# puis chargé, repli paquet), IsAccelerator, OleTranslateAccelerator et
# descripteurs de menu de l'activation sur place.
step "Incorporation OLE : depuis un fichier, accélérateurs, menus (#1109)" cargo test -q -p winlux -- classe_par_extension insertion_depuis_un_fichier accelerateurs_et_descripteurs_de_menu
