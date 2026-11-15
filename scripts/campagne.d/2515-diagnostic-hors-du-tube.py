# #2515 : les lignes de diagnostic de Winlux ne vont jamais dans la sortie
# d'erreur qu'un programme Windows donne à son enfant (l'outil de
# déploiement d'Office lisait notre historique wow32 dans son tube).
l = (ROOT / "runtime/src/log.rs").read_text(encoding="utf-8")
ok("fn init_diag_fd" in l and "fn diagnostic_hors_du_tube_du_programme" in l,
   "diagnostic de Winlux hors du tube d'un programme (#2515)")
