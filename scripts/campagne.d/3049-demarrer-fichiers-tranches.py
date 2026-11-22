# #3049 (gain) : la recherche de fichiers du menu Démarrer avance par tranches
# sur la boucle principale au lieu d'un fil qui disputait le verrou de Python
# à l'affichage. Une recherche ne lit rien avant sa première tranche, une
# nouvelle frappe arrête la recherche périmée sans rien afficher, et la
# recherche courante trouve les mêmes fichiers, dans le même ordre.
import shutil, threading, time
_menu_3049 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_3049")
_maison_3049 = Path(tempfile.mkdtemp(prefix="winlux-3049-"))
for _i in range(200):
    _d = _maison_3049 / "Documents" / f"lot {_i // 20}"; _d.mkdir(parents=True, exist_ok=True)
    (_d / f"rapport {_i}.txt").write_bytes(b"x")
(_maison_3049 / "Documents" / "Rapport final.odt").write_bytes(b"x")
# Le plus récent, donc premier des noms qui commencent par la recherche.
os.utime(_maison_3049 / "Documents" / "Rapport final.odt", (time.time() + 100, time.time() + 100))
class _Menu3049:
    def __init__(self): self.file_generation, self.file_results, self.rendus = 1, [], 0
    def render(self): self.rendus += 1
_home_3049 = os.environ.get("HOME")
os.environ["HOME"] = str(_maison_3049)
try:
    _m_3049 = _Menu3049()
    _fils_3049 = threading.active_count()
    _perime_3049 = _menu_3049["Start"].find_files(_m_3049, "rapport", 1)
    _rien_avant_3049 = _m_3049.rendus == 0 and threading.active_count() == _fils_3049
    next(_perime_3049)
    _m_3049.file_generation = 2
    _reste_3049 = sum(1 for _ in _perime_3049)
    _arret_3049 = _m_3049.rendus == 0 and _m_3049.file_results == [] and _reste_3049 == 0
    _courante_3049 = _menu_3049["Start"].find_files(_m_3049, "rapport", 2)
    _tranches_3049 = 0
    while _menu_3049["Start"].file_slice(_m_3049, _courante_3049): _tranches_3049 += 1
    _noms_3049 = [p.name for p in _m_3049.file_results]
finally:
    if _home_3049 is None: os.environ.pop("HOME", None)
    else: os.environ["HOME"] = _home_3049
    shutil.rmtree(_maison_3049, ignore_errors=True)
print(f"      rien lu avant la 1re tranche : {_rien_avant_3049} ; recherche périmée arrêtée : {_arret_3049} ; "
      f"{len(_noms_3049)} fichiers, {_m_3049.rendus} affichage(s), {_tranches_3049} tranche(s) rendue(s) ; premiers : {_noms_3049[:3]}")
ok(_rien_avant_3049 and _arret_3049 and _m_3049.rendus == 1 and len(_noms_3049) == 30
   and _noms_3049[0] == "Rapport final.odt" and all(n.lower().startswith("rapport") for n in _noms_3049),
   "Démarrer : fichiers cherchés par tranches, recherche périmée arrêtée à la frappe suivante (#3049)")
