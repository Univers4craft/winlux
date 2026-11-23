# #3217 (gain) : une ouverture du menu Démarrer ne construit sa liste qu'une
# fois (place() s'en charge ; vider une recherche restée tapée ne la
# reconstruit pas). Vérifié dans le processus du menu préchargé, sous Xvfb.
import subprocess as _sp_3217
_r_3217 = _sp_3217.run(["xvfb-run", "-a", "-s", "-screen 0 1366x768x24", "dbus-run-session",
                        "--config-file=" + str(SCRIPTS / "test-session-bus.conf"), "--",
                        sys.executable, "-B", str(SCRIPTS / "test-start-menu-rendu-unique.py")],
                       capture_output=True, text=True, timeout=180)
print("      " + "\n      ".join(l for l in _r_3217.stdout.splitlines() if l.strip()))
ok(_r_3217.returncode == 0 and "render par ouverture : [1, 1, 1, 1, 1]" in _r_3217.stdout,
   "Démarrer : la liste n'est construite qu'une fois par ouverture, recherche vidée comprise (#3217)")
