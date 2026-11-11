# #1896 : List<T>, Dictionary<TKey, TValue> et interfaces génériques dans la
# mscorlib Winlux (Corsair.Service.exe d'iCUE 4.18 quittait sur List`1::Add).
bcl_collections = (ROOT / "bcl" / "mscorlib" / "Collections.cs").read_text(encoding="utf-8")
ok("public class List<T> : IList<T>" in bcl_collections
   and "public class Dictionary<TKey, TValue>" in bcl_collections
   and "public struct Enumerator : IEnumerator<T>" in bcl_collections
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Collections.cs").read_text(encoding="utf-8"),
   "BCL C# : List<T>, Dictionary<TKey, TValue>, leurs énumérateurs et le témoin Collections (#1896)")
clr_dir = runtime_dlls.parent / "clr"
ok("pub fn find_override_of" in (clr_dir / "types.rs").read_text(encoding="utf-8")
   and "fn interface_of" in (clr_dir / "il.rs").read_text(encoding="utf-8")
   and "fn boxed_equal" in (clr_dir / "bcl.rs").read_text(encoding="utf-8"),
   "CLR : implémentations explicites d'interface et égalité des valeurs encadrées (#1896)")
