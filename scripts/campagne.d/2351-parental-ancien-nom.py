# #2351 : Mousepad bloqué avant #2248 restait affiché « Text Editor Settings »,
# dans le panneau comme dans la bulle montrée à l'enfant.
_r = subprocess.run(["xvfb-run", "-a", "-s", "-screen 0 1024x768x24",
                     os.environ.get("WINLUX_TEST_PYGTK", "/usr/bin/python3"), "-B",
                     str(SCRIPTS / "test-parental-ancien-nom.py")], capture_output=True, text=True, timeout=180)
ok(_r.returncode == 0 and "bulle de l'enfant : « Mousepad »" in _r.stdout,
   "contrôle parental : application bloquée avant la mise à jour affichée sous le nom du menu (#2351)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
