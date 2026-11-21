# Moteur .NET maison : « is » vers un type absent de notre mscorlib ne répond
# plus oui pour un objet dont toute la hiérarchie est connue, vérifié par le
# témoin TypeAbsent (code 200, #2790).
_ty = (ROOT / "runtime/src/clr/types.rs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/TypeAbsent.cs").read_text(encoding="utf-8")
ok("pub fn hierarchy_known(" in _ty and "Code de sortie attendu : 200." in _t
   and "Façade de compilation : System.Numerics." in _t,
   "CLR : un objet connu n'est pas une instance d'un type absent, témoin (#2790)")
