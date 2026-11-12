# #2018 : membres d'IntPtr et d'UIntPtr dans notre mscorlib.
# #2019 : SafeHandle, SafeFileHandle, SafeWaitHandle et leur passage aux
# appels P/Invoke ; témoin Registre.
# #2020 : Microsoft.Win32.Registry et RegistryKey au-dessus d'advapi32.
# #2023 : isinst et castclass vers T[] vérifient le type des éléments.
bcl = ROOT / "bcl" / "mscorlib"
clr = runtime_dlls.parent / "clr"
ok("public static explicit operator IntPtr(long value)" in (bcl / "Pointeurs.cs").read_text(encoding="utf-8"),
   "CLR : conversions et opérateurs d'IntPtr dans notre mscorlib (#2018)")
ok("public abstract class SafeHandle : IDisposable" in (bcl / "Poignees.cs").read_text(encoding="utf-8")
   and "fn wrap_safe_handle" in (clr / "interop.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Registre.cs").read_text(encoding="utf-8"),
   "CLR : SafeHandle et son passage aux appels P/Invoke (#2019)")
ok("public sealed class RegistryKey : IDisposable" in (bcl / "Registre.cs").read_text(encoding="utf-8")
   and "api::reg_open_key_ex_w" in (clr / "registre.rs").read_text(encoding="utf-8"),
   "CLR : Microsoft.Win32.Registry au-dessus d'advapi32 (#2020)")
ok("fn array_cast" in (clr / "tableaux.rs").read_text(encoding="utf-8"),
   "CLR : « is T[] » vérifie le type des éléments du tableau (#2023)")
