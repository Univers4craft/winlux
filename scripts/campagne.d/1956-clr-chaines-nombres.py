# #1955 / #1956 : Comparer<T>.Default, String (Format composite, Split, Join,
# indices UTF-16, constructeurs), StringBuilder, ToString/Parse des
# primitives et Char servis par notre mscorlib ; Corsair.Service (iCUE)
# s'arrêtait sur Comparer`1::get_Default.
clr = runtime_dlls.parent / "clr"
ok("public abstract class Comparer<T>" in (ROOT / "bcl" / "mscorlib" / "Comparaison.cs").read_text(encoding="utf-8")
   and "public sealed class StringBuilder" in (ROOT / "bcl" / "mscorlib" / "Chaines.cs").read_text(encoding="utf-8")
   and "internal static int ParseInteger" in (ROOT / "bcl" / "mscorlib" / "Nombres.cs").read_text(encoding="utf-8")
   and "pub fn format_double" in (clr / "nombres.rs").read_text(encoding="utf-8")
   and "pub fn new_string" in (clr / "bcl.rs").read_text(encoding="utf-8")
   and "fn same_param" in (clr / "types.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Chaines.cs").read_text(encoding="utf-8"),
   "CLR : Comparer<T>, String, StringBuilder, nombres et Char dans notre mscorlib, témoin Chaines (#1955, #1956)")
