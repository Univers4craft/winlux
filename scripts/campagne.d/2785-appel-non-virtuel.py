# Moteur .NET maison : callvirt sur une méthode non virtuelle ne prend pas une
# méthode virtuelle de même nom d'une base (#2785). Le comportement est vérifié
# par le témoin AppelNonVirtuel, compilé et exécuté par scripts/test-bcl.sh,
# qui compare son code de sortie à la ligne « Code de sortie attendu : 200. ».
_t = ROOT / "bcl/temoins/AppelNonVirtuel.cs"
ok(_t.is_file() and "Code de sortie attendu : 200." in _t.read_text(encoding="utf-8"),
   "CLR : callvirt sur une méthode non virtuelle n'en prend pas une de la base, témoin (#2785)")
