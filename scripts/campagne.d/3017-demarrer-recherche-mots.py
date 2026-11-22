# #3017 : une recherche de plusieurs mots qui nomme la catégorie
# (« paramètres son ») trouve la page des Paramètres, comme « son » seul.
_menu_3017 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_3017")
_pages_3017 = {query: [label for label, _ in _menu_3017["settings_for"](query)][:3]
               for query in ("paramètres son", "son", "settings sound", "paramètres", "son paramètres", "param")}
print(f"      pages trouvées : {_pages_3017}")
ok(_pages_3017["paramètres son"][:1] == ["Son"] and _pages_3017["son paramètres"][:1] == ["Son"]
   and _pages_3017["settings sound"][:1] == ["Son"] and _pages_3017["paramètres son"] == _pages_3017["son"],
   "Démarrer : « paramètres son » trouve la page Son comme « son » seul (#3017)")
