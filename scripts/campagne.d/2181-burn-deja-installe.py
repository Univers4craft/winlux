# Bundle Burn déjà installé : rien n'est réinstallé sans /repair ; /repair et /uninstall font l'action demandée (#2181).
_b = (ROOT / "runtime/src/setup/burn.rs").read_text(encoding="utf-8")
ok("fn installe_un_redistribuable_dans_system32(" in _b and "déjà installé, inchangé" in _b,
   "Burn : bundle déjà installé laissé tel quel, /repair le remet en place (#2181)")
