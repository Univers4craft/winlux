# #2305 : System.ComponentModel.Component et Container dans notre
# mscorlib ; Corsair.Service.exe (iCUE) démarré en session mourait sur
# MissingMethodException Component::Dispose.
bcl = ROOT / "bcl" / "mscorlib"
ok("public class Component : MarshalByRefObject, IComponent" in (bcl / "Composants.cs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Composants.cs").read_text(encoding="utf-8"),
   "CLR : System.ComponentModel.Component (#2305)")
