# #2999 : la version inscrite dans « Applications installées » l'emporte sur
# celle que la Boutique avait notée. Gardé par le scénario nommé du test.
ok("(#2999)" in (SCRIPTS / "test-store-installe-hors-boutique.py").read_text(encoding="utf-8"),
   "Boutique : version installée lue dans la clé Uninstall, scénario du test (#2999)")
