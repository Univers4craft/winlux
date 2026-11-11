# #1489 : deux applications de même nom (Claude pour Windows, Claude de la
# Boutique) sont distinguées, la plus récente est le Meilleur résultat.
start_py = (SCRIPTS / "winlux-start-menu.py").read_text(encoding="utf-8")
ok("def result_order" in start_py and "def homonyms" in start_py
   and "key=lambda app: result_order(app, query, self.state)" in start_py
   and "badge=fold(app.get_display_name()) in twins" in start_py
   and (SCRIPTS / "test-start-homonyms.py").is_file(),
   "Démarrer : homonymes distingués, la plus récente en Meilleur résultat (#1489)")
