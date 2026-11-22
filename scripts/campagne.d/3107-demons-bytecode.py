# #3107 : un démon résident lancé par winlux-python garde le nom de son script
# (lu par winlux-tray-read dans /proc/PID/comm) et son code de sortie.
import tempfile as _tf_3107
with _tf_3107.TemporaryDirectory() as _d_3107:
    _s_3107 = Path(_d_3107) / "winlux-tray"
    _s_3107.write_text("import sys\nprint(open('/proc/self/comm').read().strip(), __name__, sys.argv[1:])\nsys.exit(3)\n")
    _r_3107 = subprocess.run([sys.executable, str(SCRIPTS / "winlux-python.py"), str(_s_3107), "serve"], capture_output=True, text=True,
                             env=dict(os.environ, WINLUX_PYCACHE=str(Path(_d_3107) / "cache"), XDG_CACHE_HOME=str(Path(_d_3107) / "cache")))
    print("winlux-python :", _r_3107.returncode, _r_3107.stdout.strip(), _r_3107.stderr.strip()[-200:])
    ok(_r_3107.returncode == 3 and _r_3107.stdout.split() == ["winlux-tray", "__main__", "['serve']"],
       "winlux-python : script lancé depuis son bytecode sous son propre nom, arguments et code de sortie gardés (#3107)")
ok('"$stage/usr/lib/winlux/winlux-python"' in (SCRIPTS / "stage-desktop-components.sh").read_text(encoding="utf-8"),
   "winlux-python installé dans /usr/lib/winlux (#3107)")
