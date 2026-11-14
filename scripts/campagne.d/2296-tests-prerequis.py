# #2296 : sans PyGObject, Xvfb ou dbus-daemon, les tests de l'écran gelé et des
# bulles finissaient sur une trace Python ; ils nomment maintenant ce qui manque.
for _nom in ("test-parental-ecran-gel.py", "test-parental-bulles.py"):
    _r = subprocess.run([sys.executable, "-B", str(SCRIPTS / _nom)], capture_output=True, text=True, timeout=60,
                        env=dict(os.environ, WINLUX_TEST_PYGTK="/nonexistent/python3"))
    ok(_r.returncode == 1 and "prérequis manquant" in _r.stdout and "PyGObject" in _r.stdout
       and "Traceback" not in _r.stdout + _r.stderr,
       f"{_nom} : prérequis manquant nommé, échec sans trace Python (#2296)")
