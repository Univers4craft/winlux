# #2350 : un nom tapé dans « Choisir une application » (« mouse ») bloquait en
# silence un programme inexistant ; Mousepad restait permis.
_r = subprocess.run(["xvfb-run", "-a", "-s", "-screen 0 1024x768x24",
                     os.environ.get("WINLUX_TEST_PYGTK", "/usr/bin/python3"), "-B",
                     str(SCRIPTS / "test-parental-saisie.py")], capture_output=True, text=True, timeout=180)
ok(_r.returncode == 0 and "« mouse » + Bloquer : rien n'est enregistré" in _r.stdout,
   "contrôle parental : nom tapé inconnu refusé ou corrigé, jamais bloqué en silence (#2350)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
