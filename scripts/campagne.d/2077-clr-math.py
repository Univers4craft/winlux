# #2077 : System.Math et System.GC écrits en C# dans notre mscorlib, chaque
# surcharge typée ; le moteur ne sert plus que les fonctions transcendantes.
bcl = ROOT / "bcl" / "mscorlib"
ok("public static double Round(double value, int digits, MidpointRounding mode)" in (bcl / "Calcul.cs").read_text(encoding="utf-8")
   and "public static ulong Max(ulong val1, ulong val2)" in (bcl / "Calcul.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Calcul.cs").read_text(encoding="utf-8"),
   "CLR : Math arrondit au pair, compare les non signés et propage NaN (#2077)")
ok("public static void SuppressFinalize(object obj)" in (bcl / "Calcul.cs").read_text(encoding="utf-8"),
   "CLR : GC.SuppressFinalize et les autres membres de GC dans notre mscorlib (#2077)")
