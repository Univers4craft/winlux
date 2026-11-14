# #2361 : Exception.Data dans notre mscorlib ; Corsair.Service.exe (iCUE)
# lancé à la main mourait sur MissingMethodException Exception::get_Data.
ok("public virtual System.Collections.IDictionary Data" in (ROOT / "bcl" / "mscorlib" / "Noyau.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Donnees.cs").read_text(encoding="utf-8"),
   "CLR : Exception.Data (#2361)")
