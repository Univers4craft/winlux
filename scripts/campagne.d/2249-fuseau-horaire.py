# #2249 : un fuseau horaire tapé à la main et inconnu était ignoré sans message.
_r = subprocess.run(["xvfb-run", "-a", "-s", "-screen 0 1366x768x24",
                     os.environ.get("WINLUX_TEST_PYGTK", "/usr/bin/python3"), "-B",
                     str(SCRIPTS / "test-fuseau-horaire.py")], capture_output=True, text=True, timeout=180)
ok(_r.returncode == 0 and "« Appliquer » grisé et message affiché" in _r.stdout,
   "Heure et langue : fuseau horaire inconnu signalé, « Appliquer » grisé (#2249)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
