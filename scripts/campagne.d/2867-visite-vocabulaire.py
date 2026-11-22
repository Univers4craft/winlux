# #2867 : la visite de premier démarrage du shell ne dit plus qu'un .exe
# s'ouvre « comme sous Windows », dans aucune des quatre langues
# (vocabulaire interdit par AGENTS.md, « Marque et vocabulaire »).
visite = (SCRIPTS.parent / "testexe/src/shell/welcome.rs").read_text(encoding="utf-8")
ok(all(phrase not in visite for phrase in ("comme sous Windows", "just like on Windows",
                                          "igual que en Windows", "wie unter Windows")),
   "visite de premier démarrage : aucun « comme sous Windows » dans les textes affichés (#2867)")
ok("s'ouvre directement" in visite and "open with a double-click" in visite,
   "visite de premier démarrage : l'ouverture d'un .exe est décrite sans comparaison (#2867)")
