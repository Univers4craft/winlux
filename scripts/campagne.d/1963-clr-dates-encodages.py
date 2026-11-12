# #1963 : DateTime, DateTimeOffset, TimeSpan.ToString et Encoding (UTF-8,
# UTF-16, UTF-32, ASCII, Latin-1, 1252) dans notre mscorlib ; horloge et
# fuseau servis par le moteur.
ok("public struct DateTime" in (ROOT / "bcl" / "mscorlib" / "Dates.cs").read_text(encoding="utf-8")
   and "public abstract class Encoding" in (ROOT / "bcl" / "mscorlib" / "Encodages.cs").read_text(encoding="utf-8")
   and '"LocalOffsetTicks"' in (runtime_dlls.parent / "clr" / "bcl.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Dates.cs").read_text(encoding="utf-8"),
   "CLR : DateTime, DateTimeOffset et Encoding dans notre mscorlib, témoin Dates (#1963)")
