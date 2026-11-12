# #2026 : Marshal.SizeOf rend la taille marshalée (BOOL de 4 octets, char
# ANSI, Pack, Size, FieldOffset, ByValTStr, ByValArray), plus 8 en dur.
# #2054 : System.AppDomain dans notre mscorlib.
bcl = ROOT / "bcl" / "mscorlib"
clr = runtime_dlls.parent / "clr"
ok("fn marshal_size" in (clr / "marshal.rs").read_text(encoding="utf-8")
   and '"SizeOf" => Some(Value::I32(8))' not in (clr / "bcl.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Marshal.cs").read_text(encoding="utf-8"),
   "CLR : Marshal.SizeOf rend la taille marshalée (#2026)")
ok("public sealed class AppDomain" in (bcl / "Domaine.cs").read_text(encoding="utf-8")
   and "AppDomain.CurrentDomain.Resoudre" in (bcl / "Membres.cs").read_text(encoding="utf-8"),
   "CLR : AppDomain.CurrentDomain et AssemblyResolve (#2054)")
