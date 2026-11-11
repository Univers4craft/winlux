# #1686 : System.TimeSpan écrit en C# dans la mscorlib Winlux
# (Corsair.Service.exe d'iCUE 4.18 appelait TimeSpan.FromMilliseconds).
# #1687 : lire un champ statique de type structure ne bloque plus le CLR.
bcl_racine = ROOT / "bcl"
bcl_temps = (bcl_racine / "mscorlib" / "Temps.cs").read_text(encoding="utf-8")
ok("public struct TimeSpan" in bcl_temps and "public static TimeSpan FromMilliseconds(double value)" in bcl_temps
   and "Code de sortie attendu : 127." in (bcl_racine / "temoins" / "Durees.cs").read_text(encoding="utf-8"),
   "BCL C# : TimeSpan et son témoin Durees (#1686)")
clr_types = (runtime_dlls.parent / "clr" / "types.rs").read_text(encoding="utf-8")
ok("TYPES_HELD" in clr_types and "let size = field.info.kind.slot_size();"
   in (runtime_dlls.parent / "clr" / "il.rs").read_text(encoding="utf-8"),
   "CLR : champ statique de type structure sans verrou repris, réentrée du registre refusée (#1687)")
