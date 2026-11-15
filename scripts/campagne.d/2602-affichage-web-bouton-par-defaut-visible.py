# #2602 : dans le dialogue « Affichage web », « Plus tard » est le bouton par
# défaut visible. Le comportement est gardé par le test Xvfb nommé.
ok((SCRIPTS / "test-webview-dialogue.py").is_file(),
   "test Xvfb du bouton par défaut d'« Affichage web » (#2602)")
