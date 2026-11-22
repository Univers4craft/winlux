# Collection<T> et KeyedCollection<TKey, TItem> dans notre mscorlib (Newtonsoft
# en dérive), vérifiées par le témoin CollectionsModele (code 200, #2783).
_c = (ROOT / "bcl/mscorlib/CollectionsModele.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/CollectionsModele.cs").read_text(encoding="utf-8")
ok("public class Collection<T>" in _c and "public abstract class KeyedCollection<TKey, TItem>" in _c
   and "Code de sortie attendu : 200." in _t,
   "Collection<T> et KeyedCollection<TKey,TItem> servies par notre mscorlib, témoin (#2783)")
