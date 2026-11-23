# #3074 : choisir un bureau dans la vue des tâches y mène et ferme la vue ;
# avant, la vue restait vivante, cachée sur l'ancien bureau. Une seule vue
# par compte et par écran : la vue ouverte est retrouvée par son pid.
# Clic, second appel et nouveau bureau vérifiés sous Xvfb avec openbox par
# scripts/test-taskview-bureaux.py.
_vue_3074 = runpy.run_path(str(SCRIPTS / "winlux-taskview.py"), run_name="campagne_3074")
_dossier_3074 = Path(tempfile.mkdtemp(prefix="campagne-3074-"))
_fichier_3074 = _dossier_3074 / "winlux-taskview-0.pid"
_vivant_3074 = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)", "winlux-taskview"])
try:
    _fichier_3074.write_text(f"{_vivant_3074.pid}\n")
    _trouve_3074 = _vue_3074["running_view"](str(_fichier_3074))
    _fichier_3074.write_text(f"{os.getpid()}\n")
    _soi_3074 = _vue_3074["running_view"](str(_fichier_3074))
    _fichier_3074.write_text("999999999\n")
    _mort_3074 = _vue_3074["running_view"](str(_fichier_3074))
finally:
    _vivant_3074.kill()
    _vivant_3074.wait()
_env_3074 = dict(os.environ)
try:
    os.environ.update(XDG_RUNTIME_DIR=str(_dossier_3074), DISPLAY=":3.0")
    _chemin_3074 = _vue_3074["_pid_file"]()
finally:
    os.environ.clear()
    os.environ.update(_env_3074)
print(f"  vue vivante {_trouve_3074}, soi-même {_soi_3074}, morte {_mort_3074}, fichier {_chemin_3074}")
ok(_trouve_3074 == _vivant_3074.pid and _soi_3074 is None and _mort_3074 is None
   and _chemin_3074 == str(_dossier_3074 / "winlux-taskview-3.pid")
   and (SCRIPTS / "test-taskview-bureaux.py").exists(),
   "vue des tâches : une seule vue par compte et par écran, aucune vue fantôme (#3074)")
