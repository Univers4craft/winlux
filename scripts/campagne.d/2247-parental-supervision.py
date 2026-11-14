# #2247 : rien ne relançait le service du contrôle parental s'il s'arrêtait.
_r = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-parental-supervision.py")],
                    capture_output=True, text=True, timeout=180)
ok(_r.returncode == 0 and "service tué : relancé par la supervision" in _r.stdout
   and "winlux-parental supervise" in (SCRIPTS / "winlux-system-services.sh").read_text(encoding="utf-8"),
   "contrôle parental : service supervisé et relancé s'il s'arrête (#2247)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
