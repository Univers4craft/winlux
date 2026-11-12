# #1991 : System.Environment déclarée dans notre mscorlib, UserInteractive
# faux dans un service, GetFolderPath, OSVersion en OperatingSystem.
# #1992 : attributs d'assembly (AssemblyTitle, FileVersion, Guid,
# TargetFramework, Debuggable…) ; témoin Environnement.
# #1997 : String.Empty vaut la chaîne vide, pas null.
bcl_env = (ROOT / "bcl" / "mscorlib" / "Environnement.cs").read_text(encoding="utf-8")
ok("public static extern bool UserInteractive" in bcl_env and "public static OperatingSystem OSVersion" in bcl_env
   and "\"get_UserInteractive\"" in (runtime_dlls.parent / "clr" / "bcl.rs").read_text(encoding="utf-8"),
   "CLR : Environment.UserInteractive, GetFolderPath et OSVersion dans notre mscorlib (#1991)")
bcl_attr = (ROOT / "bcl" / "mscorlib" / "Attributs.cs").read_text(encoding="utf-8")
ok("public sealed class AssemblyFileVersionAttribute" in bcl_attr and "public sealed class TargetFrameworkAttribute" in bcl_attr
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Environnement.cs").read_text(encoding="utf-8"),
   "CLR : attributs d'assembly dans notre mscorlib, témoin Environnement (#1992)")
ok("fn is_string_empty" in (runtime_dlls.parent / "clr" / "il.rs").read_text(encoding="utf-8"),
   "CLR : String.Empty vaut la chaîne vide (#1997)")
