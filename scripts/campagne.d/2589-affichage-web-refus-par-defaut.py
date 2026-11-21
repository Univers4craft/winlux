# #2589 : le dialogue « Affichage web » refuse le téléchargement par défaut.
# Comportement vérifié sous Xvfb par scripts/test-webview-dialogue.py (le
# vrai dialogue GTK : « Plus tard » est le bouton par défaut et une réponse
# « Plus tard » ne lance aucune installation) ; ce contrôle en vérifie le nom.
ok((SCRIPTS / "test-webview-dialogue.py").is_file(),
   "Affichage web : « Plus tard » est la réponse par défaut, test Xvfb (#2589)")
