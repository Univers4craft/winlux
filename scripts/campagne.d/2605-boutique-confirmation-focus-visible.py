# #2605 : dans « Installer … ? » de la Boutique, « Annuler » est le bouton
# actif visible dès l'ouverture. Le comportement est gardé par le test Xvfb nommé.
ok((SCRIPTS / "test-store-confirm-dialogue.py").is_file(),
   "test Xvfb du bouton actif de la confirmation de la Boutique (#2605)")
