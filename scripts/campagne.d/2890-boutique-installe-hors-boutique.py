# #2890 : un programme Windows installé hors de la Boutique compte comme
# installé sur sa fiche (jamais de rétrogradation). Comportement gardé par le
# test nommé.
ok((SCRIPTS / "test-store-installe-hors-boutique.py").is_file(),
   "Boutique : programme Windows installé hors Boutique vu comme installé, test (#2890)")
