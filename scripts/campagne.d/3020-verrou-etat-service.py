# Paramètres > Confidentialité et sécurité : le verrouillage automatique
# affiché suit le service qui verrouille vraiment, pas seulement lock.conf
# (#3020). Vrai `winlux-lock daemon` lancé dans un dossier isolé, écran :77.
import shutil as _sh3020, signal as _sig3020, time as _t3020
_d3020 = Path(tempfile.mkdtemp())
(_d3020 / "etc/winlux").mkdir(parents=True)
(_d3020 / "run/winlux").mkdir(parents=True)
(_d3020 / "bin").mkdir()
_sh3020.copy(SCRIPTS / "winlux-lock.py", _d3020 / "bin/winlux-lock")
_env3020 = dict(os.environ, WINLUX_LOCK_ROOT=str(_d3020), DISPLAY=":77", WINLUX_LANG="fr",
                WINLUX_SESSION_USER="personne-3020")
def _etat3020():
    out = subprocess.run([sys.executable, "-I", str(_d3020 / "bin/winlux-lock"), "status"], env=_env3020,
                         capture_output=True, text=True, timeout=20).stdout
    return dict(line.split("=", 1) for line in out.splitlines() if "=" in line)
_sans3020 = _etat3020()
_svc3020 = subprocess.Popen([sys.executable, "-I", str(_d3020 / "bin/winlux-lock"), "daemon"], env=_env3020,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    for _i3020 in range(50):
        _avec3020 = _etat3020()
        if _avec3020.get("lock_service") == "1": break
        _t3020.sleep(0.1)
    _double3020 = subprocess.run([sys.executable, "-I", str(_d3020 / "bin/winlux-lock"), "daemon"], env=_env3020,
                                 capture_output=True, text=True, timeout=20)
finally:
    _svc3020.send_signal(_sig3020.SIGTERM)
    _svc3020.wait(timeout=10)
_apres3020 = _etat3020()
print("#3020 sans service :", _sans3020, "| avec :", _avec3020, "| après arrêt :", _apres3020,
      "| second service :", _double3020.returncode, _double3020.stderr.strip())
ok(_sans3020.get("lock_service") == "0" and _avec3020.get("lock_service") == "1"
   and _apres3020.get("lock_service") == "0" and _avec3020.get("lock_after") == "300",
   "winlux-lock status dit si le service de verrouillage de cet écran tourne (#3020)")
ok(_double3020.returncode == 0 and "déjà lancé" in _double3020.stderr,
   "un second winlux-lock daemon sur le même écran s'arrête au lieu de doubler le service (#3020)")
with patch.dict(os.environ, {"WINLUX_LANG": "fr"}):
    _p3020 = runpy.run_path(str(SCRIPTS / "winlux-settings.py"), run_name="test_campaign")
_base3020 = {"lock_after": "300", "lock_on_resume": "1", "lock_account": "damien", "lock_password": "1"}
_arret3020 = _p3020["privacy_summary"](dict(_base3020, lock_service="0"))
_marche3020 = _p3020["privacy_summary"](dict(_base3020, lock_service="1"))
print("#3020 encadré service arrêté :", _arret3020.replace("\n", " · "))
print("#3020 encadré service en marche :", _marche3020.replace("\n", " · "))
ok("Verrouillage automatique : désactivé" in _arret3020 and "5 minutes" not in _arret3020
   and "Verrouillage automatique : 5 minutes" in _marche3020 and "désactivé" not in _marche3020,
   "Confidentialité et sécurité : verrouillage « désactivé » quand le service est arrêté, délai lu sinon (#3020)")
_sh3020.rmtree(_d3020, ignore_errors=True)
