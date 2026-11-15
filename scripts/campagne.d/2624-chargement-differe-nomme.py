# #2624 : une exception de chargement différé non gérée (0xC06D007E/7F)
# nomme la DLL, la fonction et l'erreur lues dans sa DelayLoadInfo
# (l'outil de déploiement d'Office mourait sans nommer son module, #2623).
k = (ROOT / "runtime/src/dlls/kernel32_misc.rs").read_text(encoding="utf-8")
ok("fn delay_load_failure" in k and "fn chargement_differe_nomme_la_dll" in k,
   "Chargement différé : DLL manquante nommée au journal (#2624)")
