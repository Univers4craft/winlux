# #1975 : LINQ to Objects (System.Linq.Enumerable), Func/Action jusqu'à 16
# paramètres et Tuple dans notre mscorlib ; témoin Linq.
# #1977 : un tableau sert IEnumerable<T>, ICollection<T> et IList<T>.
# #1978 : « is »/« as » vers une interface suivent la table InterfaceImpl.
bcl_linq = (ROOT / "bcl" / "mscorlib" / "Linq.cs").read_text(encoding="utf-8")
ok("public static class Enumerable" in bcl_linq and "public interface IGrouping<out TKey, out TElement>" in bcl_linq
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Linq.cs").read_text(encoding="utf-8"),
   "CLR : LINQ to Objects et Tuple dans notre mscorlib, témoin Linq (#1975)")
ok("fn array_interface_call" in (runtime_dlls.parent / "clr" / "tableaux.rs").read_text(encoding="utf-8"),
   "CLR : un tableau T[] sert IEnumerable<T>, ICollection<T> et IList<T> (#1977)")
ok("fn implements" in (runtime_dlls.parent / "clr" / "typesys.rs").read_text(encoding="utf-8")
   and "self.implements(actual, target)" in (runtime_dlls.parent / "clr" / "il.rs").read_text(encoding="utf-8"),
   "CLR : isinst/castclass vers une interface suivent InterfaceImpl (#1978)")
