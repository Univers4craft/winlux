# Barre des tâches recalculée sur changement (#3129) : immobile, le service
# ne relance plus xwininfo, xprop, nmcli ni pactl toutes les 2 à 3 s.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-tray-changements.py")],
                    capture_output=True, text=True, timeout=300)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "barre recalculée sur changement : OK" in _r.stdout,
   "barre des tâches recalculée seulement sur changement (#3129)")
