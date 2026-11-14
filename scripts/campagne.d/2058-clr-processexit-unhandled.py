# #2058 : le moteur lève AppDomain.ProcessExit (retour de Main,
# Environment.Exit) et UnhandledException (exception non gérée).
clr = ROOT / "runtime" / "src" / "clr"
ok('raise_domain_event("LeverSortie"' in (clr / "mod.rs").read_text(encoding="utf-8")
   and 'raise_domain_event("LeverExceptionNonGeree"' in (clr / "mod.rs").read_text(encoding="utf-8")
   and "AppDomain.LeverSortie();" in (ROOT / "bcl" / "mscorlib" / "Environnement.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Sortie.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "NonGeree.cs").read_text(encoding="utf-8"),
   "CLR : AppDomain.ProcessExit et UnhandledException levés (#2058)")
