# #3068 : la recherche du menu Démarrer tolère une faute de frappe et le
# milieu d'un mot dans le nom d'une application (annonce Microsoft du 13 juillet
# 2026 : fautes de frappe pour les applications ; seuils et milieu de mot à
# nous), toujours après les correspondances exactes. « rapp » ne trouve pas
# « Application » (le début comparé a 4 lettres au moins). Les Paramètres gardent le
# début de mot.
_menu_3068 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_3068")
class _App3068:
    def __init__(self, name, keywords=()): self.name, self.keywords = name, list(keywords)
    def get_display_name(self): return self.name
    def get_keywords(self): return self.keywords
    def get_generic_name(self): return ""
    def get_id(self): return self.name
_apps_3068 = [_App3068(n) for n in ("Outlook", "Notepad++", "Mousepad", "Calculatrice", "LibreOffice Calc", "Steam", "Paint.NET", "Padlet", "Application 001")]
def _cherche_3068(texte):
    q = _menu_3068["fold"](texte)
    return [a.name for a in sorted((a for a in _apps_3068 if _menu_3068["matches"](a, q)),
                                   key=lambda a: _menu_3068["result_order"](a, q, {}))]
_vus_3068 = {t: _cherche_3068(t) for t in ("outlok", "utlook", "ntoepad", "clac", "pad", "teams", "xyz", "ou", "rapp")}
_param_3068 = [label for label, _ in _menu_3068["settings_for"](_menu_3068["fold"]("afichage"))]
_param_exact_3068 = [label for label, _ in _menu_3068["settings_for"](_menu_3068["fold"]("affichage"))]
for _t, _r in _vus_3068.items(): print(f"      « {_t} » : {_r}")
print(f"      Paramètres « afichage » : {_param_3068} ; « affichage » : {_param_exact_3068}")
ok(_vus_3068["outlok"] == ["Outlook"] and _vus_3068["utlook"] == ["Outlook"] and _vus_3068["ntoepad"] == ["Notepad++"]
   and sorted(_vus_3068["clac"]) == ["Calculatrice", "LibreOffice Calc"],
   "Démarrer : une faute de frappe dans le nom d'une application la trouve encore (#3068)")
ok(_vus_3068["pad"][0] == "Padlet" and set(_vus_3068["pad"][1:]) == {"Mousepad", "Notepad++"}
   and _vus_3068["teams"] == [] and _vus_3068["rapp"] == [] and _vus_3068["xyz"] == [] and _vus_3068["ou"] == ["Outlook"] and _param_3068 == [] and _param_exact_3068 != [],
   "Démarrer : exact avant milieu de mot, rien de lointain, Paramètres au début de mot seulement (#3068)")
