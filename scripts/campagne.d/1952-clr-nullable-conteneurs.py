# #1952 : Nullable<T> (encadrement ECMA-335 compris), EventHandler, Queue,
# Stack, HashSet, LinkedList et ConcurrentDictionary servis par notre
# mscorlib ; Corsair.Service (iCUE) les appelait.
ok("fn nullable_parts" in (runtime_dlls.parent / "clr" / "typesys.rs").read_text(encoding="utf-8")
   and "public struct Nullable<T>" in (ROOT / "bcl" / "mscorlib" / "Valeurs.cs").read_text(encoding="utf-8")
   and "public class ConcurrentDictionary<TKey, TValue>" in (ROOT / "bcl" / "mscorlib" / "Conteneurs.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Conteneurs.cs").read_text(encoding="utf-8"),
   "CLR : Nullable, EventHandler, Queue, Stack, HashSet, LinkedList, ConcurrentDictionary dans notre mscorlib, témoin Conteneurs (#1952)")
