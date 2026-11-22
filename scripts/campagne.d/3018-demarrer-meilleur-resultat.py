# #3018 : « écran » met la page Paramètres « Écran » en Meilleur résultat
# avant « Copier le texte de l'écran » ; à rang égal l'application reste en
# tête (« paramètres »). Comportement de l'affichage et d'Entrée vérifié sous
# Xvfb par scripts/test-start-search-meilleur.py (CI et test-all).
_menu_3018 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_3018")
class _App3018:
    def __init__(self, name): self.name = name
    def get_display_name(self): return self.name
_premier_3018 = _menu_3018.get("page_first", lambda apps, pages, query: False)
_cas_3018 = {}
for _requete, _appli in (("écran", "Copier le texte de l'écran"), ("paramètres", "Paramètres"), ("son", "Sonnerie")):
    _q = _menu_3018["fold"](_requete)
    _pages = _menu_3018["settings_for"](_q)
    _cas_3018[_requete] = (_pages[0][0] if _pages else None, _premier_3018([_App3018(_appli)], _pages, _q))
print(f"      première page et page en tête : {_cas_3018}")
ok(_cas_3018["écran"] == ("Écran", True) and _cas_3018["paramètres"][1] is False and _cas_3018["son"][1] is False
   and (SCRIPTS / "test-start-search-meilleur.py").exists(),
   "Démarrer : « écran » met la page Paramètres Écran en Meilleur résultat, à rang égal l'application reste en tête (#3018)")
