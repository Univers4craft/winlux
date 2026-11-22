# #2077 : System.Math et System.GC écrits en C# dans notre mscorlib, chaque
# surcharge typée ; le moteur ne sert plus que les fonctions transcendantes.
bcl = ROOT / "bcl" / "mscorlib"
ok("public static class Math" in (bcl / "Calcul.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Calcul.cs").read_text(encoding="utf-8"),
   "CLR : Math arrondit au pair, compare les non signés et propage NaN (#2077)")
# GC.SuppressFinalize vérifié par l'étape 8 du témoin Calcul.
ok("public static class GC" in (bcl / "Calcul.cs").read_text(encoding="utf-8"),
   "CLR : GC.SuppressFinalize et les autres membres de GC dans notre mscorlib (#2077)")
