# #3196 : le démon du catalogue ne décode pas les icônes lui-même (glycin, X11).
_r_3196 = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-icones-fils.py")], capture_output=True, text=True, timeout=60)
print(_r_3196.stdout + _r_3196.stderr)
ok(_r_3196.returncode == 0, "catalogue : icônes des fenêtres décodées dans un fils, démon sans GdkPixbuf ni X11 (#3196)")
