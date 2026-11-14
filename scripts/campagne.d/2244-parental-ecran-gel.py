# #2244 : l'écran de blocage relancé après le gel de la session (deuxième
# fermeture forcée) attendait le bus d'accessibilité gelé du compte et
# n'affichait jamais sa fenêtre. Vrai Xvfb, faux bus gelé annoncé par AT_SPI_BUS.
_r = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-parental-ecran-gel.py")],
                    capture_output=True, text=True, timeout=180)
ok(_r.returncode == 0 and "écran de blocage affiché en plein écran" in _r.stdout,
   "contrôle parental : écran de blocage affiché malgré le bus d'accessibilité gelé (#2244)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
