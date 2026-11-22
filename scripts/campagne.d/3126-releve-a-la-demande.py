# Relevé d'état à la demande (#3126) : un bureau immobile ne lance plus
# winlux-control toutes les 3 s ; une demande donne exactement un relevé.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-releve-a-la-demande.py")],
                    capture_output=True, text=True, timeout=120)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "relevé à la demande : OK" in _r.stdout,
   "relevé d'état à la demande, aucun programme lancé sans demande (#3126)")
