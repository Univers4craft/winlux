# #2858 : le volet « Pour vous » ne relit recently-used.xbel que s'il a changé
# et se calcule une fois le menu affiché ; l'ouverture du menu préchargé ne
# dépend plus de l'historique. Comportement mesuré sous Xvfb par
# scripts/test-start-menu-ouverture.py (CI et test-all).
_menu_2858 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_2858")
_home_2858 = Path(tempfile.mkdtemp(prefix="campagne-2858-"))
(_home_2858 / "a.txt").write_text("a")
(_home_2858 / "b.txt").write_text("b")
_xbel_2858 = _home_2858 / "recently-used.xbel"
def _ecrit_2858(*noms):
    _xbel_2858.write_text('<xbel version="1.0">' + "".join(
        f'<bookmark href="file://{_home_2858}/{nom}" added="2026-11-{1 + i % 28:02d}T10:00:00Z"/>'
        for i, nom in enumerate(noms)) + "</xbel>")
_ecrit_2858("a.txt")
_avant_2858 = [p.name for p, _, _ in _menu_2858["recent_files"](_xbel_2858, home=_home_2858)]
_ecrit_2858("a.txt", "b.txt")
_apres_2858 = [p.name for p, _, _ in _menu_2858["recent_files"](_xbel_2858, home=_home_2858)]
# Historique inchangé : la seconde lecture ne réanalyse pas les 1 000 entrées.
for _i in range(1000): (_home_2858 / f"d{_i}.txt").write_text("x")
_ecrit_2858(*[f"d{_i}.txt" for _i in range(1000)])
import time as _time_2858
_t0 = _time_2858.perf_counter(); _menu_2858["recent_files"](_xbel_2858, home=_home_2858)
_t1 = _time_2858.perf_counter(); _menu_2858["recent_files"](_xbel_2858, home=_home_2858)
_t2 = _time_2858.perf_counter()
ok((_t2 - _t1) * 5 < (_t1 - _t0),
   f"Démarrer : historique de 1 000 fichiers inchangé, pas réanalysé à l'ouverture suivante "
   f"({(_t1 - _t0) * 1000:.0f} ms puis {(_t2 - _t1) * 1000:.1f} ms) (#2858)")
ok(_avant_2858 == ["a.txt"] and _apres_2858 == ["b.txt", "a.txt"]
   and (SCRIPTS / "test-start-menu-ouverture.py").exists(),
   "Démarrer : historique relu quand il change, ouverture mesurée indépendante de l'historique (#2858)")
