# #3285 : Process.MainWindowHandle et MainWindowTitle rendent la vraie fenêtre
# principale (annuaire des fenêtres du compte) au lieu de zéro et "". Le
# comportement est gardé par le témoin nommé, lancé par scripts/test-bcl.sh.
t = ROOT / "bcl" / "temoins" / "FenetrePrincipale.cs"
texte = t.read_text(encoding="utf-8") if t.exists() else ""
ok("Code de sortie attendu : 200." in texte, "témoin FenetrePrincipale : MainWindowHandle et MainWindowTitle réels (#3285)")
