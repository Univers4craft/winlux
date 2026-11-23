# #3040 (gain) : la recherche du menu Démarrer ne replie plus à chaque frappe
# les noms des applications et des pages des Paramètres : une seconde frappe
# sur les mêmes textes ne rappelle pas fold(), et les résultats ne changent
# pas. Les noms de fichiers, eux, ne passent pas par le cache.
_menu_3040 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_3040")
class _App3040:
    def __init__(self, name, keywords=()): self.name, self.keywords = name, list(keywords)
    def get_display_name(self): return self.name
    def get_keywords(self): return self.keywords
    def get_generic_name(self): return "Éditeur de texte"
_apps_3040 = [_App3040("Notepad++", ["texte"]), _App3040("Paint.NET"), _App3040("Gestionnaire des tâches")]
_appels_3040 = [0]
_fold_3040 = _menu_3040["fold"]
def _compte_3040(text):
    _appels_3040[0] += 1
    return _fold_3040(text)
_menu_3040["matches"].__globals__["fold"] = _compte_3040
def _frappe_3040(query):
    _appels_3040[0] = 0
    found = [a.name for a in _apps_3040 if _menu_3040["matches"](a, query)] + [l for l, _ in _menu_3040["settings_for"](query)]
    return found, _appels_3040[0]
_premiere_3040, _ = _frappe_3040("note")
_seconde_3040, _replis_3040 = _frappe_3040("note")
_fichier_3040 = _menu_3040["found_in"]("note", "Notes de cours.txt", cache=False) if "cache" in _menu_3040["found_in"].__code__.co_varnames else None
print(f"      « note » : {_seconde_3040}, fold() appelé {_replis_3040} fois à la seconde frappe")
ok(_premiere_3040 == _seconde_3040 and "Notepad++" in _seconde_3040 and _replis_3040 <= 2 and _fichier_3040 is True,
   "Démarrer : noms des applications et des Paramètres repliés une seule fois, pas à chaque frappe (#3040)")
