# #2994 : le menu Démarrer sait que winlux-lock tient l'écran (pid vivant
# dans /run/winlux/lock/active) et ne s'ouvre pas par-dessus. Ouverture
# refusée vérifiée sous Xvfb par scripts/test-start-menu-verrou.py.
_menu_2994 = runpy.run_path(str(SCRIPTS / "winlux-start-menu.py"), run_name="campagne_2994")
_racine_2994 = Path(tempfile.mkdtemp(prefix="campagne-2994-"))
(_racine_2994 / "run/winlux/lock").mkdir(parents=True)
_marqueur_2994 = _racine_2994 / "run/winlux/lock/active"
_env_2994 = dict(os.environ)
try:
    os.environ.update(WINLUX_LOCK_ROOT=str(_racine_2994), DISPLAY=":0")
    _sans_2994 = _menu_2994["screen_locked"]()
    _marqueur_2994.write_text(f"{os.getpid()}\n")
    _verrou_2994 = _menu_2994["screen_locked"]()
    _marqueur_2994.write_text("999999999\n")
    _mort_2994 = _menu_2994["screen_locked"]()
finally:
    os.environ.clear()
    os.environ.update(_env_2994)
ok(not _sans_2994 and _verrou_2994 and not _mort_2994
   and (SCRIPTS / "test-start-menu-verrou.py").exists(),
   "Démarrer : écran verrouillé reconnu, le menu ne s'ouvre pas par-dessus (#2994)")
