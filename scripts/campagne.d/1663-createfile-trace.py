# CreateFile dit dans la trace quel fichier il crée, refuse ou efface à la fermeture (#1663).
k32 = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
ok("fichier créé" in k32 and "effacé à la fermeture" in k32 and "(#1663)" in k32,
   "CreateFile : création, échec et effacement à la fermeture notés sous WINLUX_TRACE (#1663)")
