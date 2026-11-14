# #2383 : un rappel natif arrivé sur un fil créé par le code natif
# (CreateThread, ServiceMain) entre dans le runtime au lieu de rendre 0.
ok("fn dispatch_foreign" in (ROOT / "runtime" / "src" / "clr" / "callbacks.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "FilNatif.cs").read_text(encoding="utf-8"),
   "CLR : rappel natif depuis un fil CreateThread (#2383)")
