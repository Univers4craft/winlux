# #2745 : la fenêtre du Terminal Winlux restait ouverte quand bash perdait le
# SIGHUP envoyé par xterm à la fermeture. Le shell par défaut passe par le
# relais winlux-terminal-shell, posé dans l'image ; le comportement est gardé
# par scripts/test-terminal-fermeture.py, lancé par l'étape test-all 2745.
etape_2745 = (ROOT / "scripts" / "test-all.d" / "2745-terminal-fermeture.sh").read_text(encoding="utf-8")
relais_2745 = ROOT / "integration" / "winlux-terminal-shell"
pose_2745 = (ROOT / "scripts" / "stage-terminal.sh").read_text(encoding="utf-8")
lanceur_2745 = (ROOT / "integration" / "winlux-terminal").read_text(encoding="utf-8")
ok("test-terminal-fermeture.py" in etape_2745 and relais_2745.is_file() and os.access(relais_2745, os.X_OK)
   and "/usr/lib/winlux/winlux-terminal-shell" in pose_2745 and "/usr/lib/winlux/winlux-terminal-shell" in lanceur_2745,
   "Terminal Winlux : fermer la fenêtre termine le shell, même occupé (#2745)")
