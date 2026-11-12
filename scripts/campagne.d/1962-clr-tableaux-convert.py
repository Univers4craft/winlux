# #1962 : Array.Copy, Clear, Reverse, Clone et CreateInstance servis par le
# moteur (Copy ne copiait rien), méthodes génériques d'Array, BitConverter,
# Guid et Convert dans notre mscorlib.
clr = runtime_dlls.parent / "clr"
ok("fn array_copy" in (clr / "tableaux.rs").read_text(encoding="utf-8")
   and "self.array_call(" in (clr / "il.rs").read_text(encoding="utf-8")
   and "public static class BitConverter" in (ROOT / "bcl" / "mscorlib" / "Tableaux.cs").read_text(encoding="utf-8")
   and "public static class Convert" in (ROOT / "bcl" / "mscorlib" / "Conversion.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Tableaux.cs").read_text(encoding="utf-8"),
   "CLR : Array.Copy et méthodes d'Array, BitConverter, Guid et Convert, témoin Tableaux (#1962)")
