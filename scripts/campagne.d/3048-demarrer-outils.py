# #3048 : « Toutes les applications » a une catégorie « Outils » (comme
# « Utilitaires et outils » de Windows 11) ; le terminal y est rangé.
_menu_3048 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_3048")
class _Appli3048:
    def __init__(self, ident, categories): self.ident, self.categories = ident, categories
    def get_id(self): return self.ident
    def get_categories(self): return self.categories
_rangs_3048 = {ident: _menu_3048["category"](_Appli3048(ident, cats)) for ident, cats in
               [("winlux-terminal.desktop", "System;TerminalEmulator;"), ("winlux-capture.desktop", "Utility;"),
                ("winlux-taskmgr.desktop", "System;")]}
print(f"      catégories : {_rangs_3048}")
ok(_rangs_3048 == {"winlux-terminal.desktop": "Outils", "winlux-capture.desktop": "Outils",
                   "winlux-taskmgr.desktop": "Système"}
   and "Outils" in _menu_3048["CATEGORY_ORDER"] and "Accessoires" not in _menu_3048["CATEGORY_ORDER"],
   "Démarrer : catégorie « Outils » avec le terminal et les utilitaires (#3048)")
