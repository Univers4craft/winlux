# #2894 : Ctrl+V colle le presse-papiers dans la recherche de la Boutique.
# Comportement gardé par le test Xvfb nommé (vrai Ctrl+V par XTest).
ok((SCRIPTS / "test-store-coller-xvfb.py").is_file(),
   "Boutique : Ctrl+V colle dans la recherche, test Xvfb (#2894)")
