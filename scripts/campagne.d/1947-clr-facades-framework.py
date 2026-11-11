# #1947 : un type référencé dans une assembly du .NET Framework non
# chargée (System, System.Core…) est cherché dans notre mscorlib : Stopwatch
# de System.dll n'atteignait pas notre Stopwatch.
ok("pub fn is_framework_assembly" in (runtime_dlls.parent / "clr" / "domain.rs").read_text(encoding="utf-8")
   and "is_framework_assembly(&wanted)" in (runtime_dlls.parent / "clr" / "typesys.rs").read_text(encoding="utf-8")
   and "Façade de compilation : System." in (ROOT / "bcl" / "temoins" / "Facade.cs").read_text(encoding="utf-8")
   and (ROOT / "bcl" / "facades" / "System.cs").exists(),
   "CLR : types de System.dll et des autres assemblies du Framework servis par notre mscorlib, témoin Facade (#1947)")
