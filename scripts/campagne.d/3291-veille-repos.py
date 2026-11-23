# Veille de l'écran au repos (#3291) : update_busy répond sans lancer de
# programme, et le démon XScreenSaver connu ne fait plus relire tout /proc.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-veille-repos.py")],
                    capture_output=True, text=True, timeout=300)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "veille au repos : OK" in _r.stdout,
   "veille de l'écran au repos : aucun programme lancé, /proc non relu (#3291)")
