# #1884 : Type.GetType(string[, throwOnError[, ignoreCase]]) (Corsair.Service.exe
# d'iCUE 4.18 quittait sur MissingMethodException System.Type::GetType).
bcl_reflexion = (ROOT / "bcl" / "mscorlib" / "Reflexion.cs").read_text(encoding="utf-8")
ok("public static extern Type GetType(string typeName, bool throwOnError, bool ignoreCase);" in bcl_reflexion
   and "public class TypeLoadException" in bcl_reflexion
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "TypeParNom.cs").read_text(encoding="utf-8"),
   "BCL C# : surcharges statiques de Type.GetType et leur témoin TypeParNom (#1884)")
clr_reflexion = (runtime_dlls.parent / "clr" / "reflection.rs").read_text(encoding="utf-8")
ok("fn type_by_name" in clr_reflexion and "fn split_qualified" in clr_reflexion and "fn find_type_in" in clr_reflexion,
   "CLR : Type.GetType cherche l'assembly appelante, mscorlib ou l'assembly nommée, types imbriqués compris (#1884)")
