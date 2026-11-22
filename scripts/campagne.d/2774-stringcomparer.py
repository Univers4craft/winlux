# System.StringComparer (Ordinal, IgnoreCase, culture, Create) dans notre
# mscorlib, vérifié par le témoin ComparateursChaines (code 200, #2774).
_c = (ROOT / "bcl/mscorlib/ComparateursChaines.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/ComparateursChaines.cs").read_text(encoding="utf-8")
ok("public abstract class StringComparer" in _c and "public interface IEqualityComparer" in _c
   and "Code de sortie attendu : 200." in _t,
   "StringComparer et IEqualityComparer servis par notre mscorlib, témoin (#2774)")
