# Moteur .NET maison : callvirt sur une méthode non virtuelle ne prend pas une
# méthode virtuelle de même nom d'une base, vérifié par le témoin
# AppelNonVirtuel (code 200, #2785).
_il = (ROOT / "runtime/src/clr/il.rs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/AppelNonVirtuel.cs").read_text(encoding="utf-8")
ok("reg.def_in_chain(decl, declared)" in _il and "Code de sortie attendu : 200." in _t,
   "CLR : callvirt sur une méthode non virtuelle n'en prend pas une de la base, témoin (#2785)")
