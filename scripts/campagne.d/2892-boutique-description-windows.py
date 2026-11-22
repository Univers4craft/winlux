# #2892 : la fiche d'un programme Windows n'a jamais de section
# « Description » vide (le résumé la remplace). Gardé par le test nommé.
ok((SCRIPTS / "test-store-description-windows.py").is_file(),
   "Boutique : description d'une fiche Windows jamais vide, test (#2892)")
