# Moteur .NET maison : une méthode introuvable est journalisée avec l'appelant
# et le type réel de `this`, vérifié par un vrai programme .NET (#2780).
_il = (ROOT / "runtime/src/clr/il.rs").read_text(encoding="utf-8")
_s = (ROOT / "scripts/test-clr-methode-introuvable.sh").read_text(encoding="utf-8")
ok("méthode introuvable {}.{}::{} appelée par {}, this : {}" in _il
   and "this : System.Windows.Forms.EssaiAbsent" in _s
   and (ROOT / "scripts/test-all.d/2780-clr-methode-introuvable.sh").exists(),
   "CLR : méthode introuvable journalisée avec appelant et type de this (#2780)")
