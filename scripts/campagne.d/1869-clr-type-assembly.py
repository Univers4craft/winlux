# #1869 : Type.Assembly et System.Reflection.Assembly (Corsair.Service.exe
# d'iCUE 4.18 quittait sur MissingMethodException System.Type::get_Assembly).
bcl_reflexion = (ROOT / "bcl" / "mscorlib" / "Reflexion.cs").read_text(encoding="utf-8")
ok("public extern virtual Assembly Assembly" in bcl_reflexion
   and "public virtual AssemblyName GetName()" in bcl_reflexion
   and "public sealed class Version" in bcl_reflexion
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Reflexion.cs").read_text(encoding="utf-8"),
   "BCL C# : Type.Assembly, Assembly.GetName, Version et leur témoin Reflexion (#1869)")
clr_reflexion = (runtime_dlls.parent / "clr" / "reflection.rs").read_text(encoding="utf-8")
ok("\"get_Assembly\" =>" in clr_reflexion and "\"GetExecutingAssembly\"" in clr_reflexion
   and "pub path: Option<PathBuf>," in (runtime_dlls.parent / "clr" / "domain.rs").read_text(encoding="utf-8"),
   "CLR : un objet Type et un objet Assembly uniques, avec nom, version et emplacement (#1869)")
