# #1922 : System.Diagnostics.Stopwatch dans la mscorlib Winlux (Corsair.Service.exe
# d'iCUE 4.18 quittait sur Stopwatch::StartNew), et relevé d'un coup des méthodes
# .NET non servies (WINLUX_CLR_MANQUANTS, l'équivalent de manquants.log).
ok("public static Stopwatch StartNew()" in (ROOT / "bcl" / "mscorlib" / "Chrono.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Chrono.cs").read_text(encoding="utf-8")
   and "\"Stopwatch\", \"GetTimestamp\"" in (runtime_dlls.parent / "clr" / "bcl.rs").read_text(encoding="utf-8"),
   "BCL C# : Stopwatch sur l'horloge monotone du moteur, et son témoin Chrono (#1922)")
ok("fn report_missing" in (runtime_dlls.parent / "clr" / "resolve.rs").read_text(encoding="utf-8")
   and "WINLUX_CLR_MANQUANTS" in (runtime_dlls.parent / "clr" / "mod.rs").read_text(encoding="utf-8"),
   "CLR : WINLUX_CLR_MANQUANTS liste d'un coup les méthodes .NET référencées et non servies (#1922)")
