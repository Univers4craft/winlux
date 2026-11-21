# #1489 : deux applications de même nom (Claude pour Windows, Claude de la
# Boutique) sont distinguées, la plus récente est le Meilleur résultat.
start_py = (SCRIPTS / "winlux-start-menu.py").read_text(encoding="utf-8")
ok("def result_order" in start_py and "def homonyms" in start_py
   and "result_order(" in start_py
   and "fold(" in start_py
   and (SCRIPTS / "test-start-homonyms.py").is_file(),
   "Démarrer : homonymes distingués, la plus récente en Meilleur résultat (#1489)")
