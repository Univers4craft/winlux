# #2233 : Environment.GetEnvironmentVariables (Hashtable, IDictionary non
# générique) et Console.CursorLeft dans notre mscorlib ; l'installeur de
# Razer Synapse 4 s'arrêtait sur MissingMethodException.
bcl = ROOT / "bcl" / "mscorlib"
ok("public static System.Collections.IDictionary GetEnvironmentVariables()" in (bcl / "Environnement.cs").read_text(encoding="utf-8")
   and "public class Hashtable : IDictionary" in (bcl / "Dictionnaires.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Dictionnaires.cs").read_text(encoding="utf-8"),
   "CLR : Environment.GetEnvironmentVariables rend une Hashtable (#2233)")
ok("public static int CursorLeft" in (bcl / "Noyau.cs").read_text(encoding="utf-8"),
   "CLR : Console.CursorLeft, IOException sans console (#2233)")
