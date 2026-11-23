# Process.WaitForInputIdle passe par WaitForInputIdle de user32 (#3287).
t = ROOT / "bcl" / "temoins" / "AttenteDeSaisie.cs"
texte = t.read_text(encoding="utf-8") if t.exists() else ""
ok("Code de sortie attendu : 200." in texte, "témoin AttenteDeSaisie : WaitForInputIdle regarde le processus (#3287)")
