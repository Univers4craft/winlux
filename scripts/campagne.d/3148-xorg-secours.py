# Xorg qui ne démarre pas (#3148) : seconde tentative sans la configuration
# générée par Winlux, puis un message et le bouton d'alimentation servi ;
# jamais de console root sans mot de passe sur un système installé.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-xorg-secours.py")],
                    capture_output=True, text=True, timeout=300)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "Xorg en échec : OK" in _r.stdout,
   "Xorg en échec : seconde tentative, message, aucune console root (#3148)")
