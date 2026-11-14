# #2248 : « Choisir une application » proposait « Text Editor Settings » au
# lieu de Mousepad, et des lanceurs cachés du menu (gcr-prompter, nm-applet).
_r = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-parental-lanceurs.py")],
                    capture_output=True, text=True, timeout=60)
ok(_r.returncode == 0 and "Mousepad proposé sous son nom" in _r.stdout,
   "contrôle parental : applications proposées sous le nom du menu, sans lanceur caché (#2248)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
