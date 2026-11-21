# #2730 : WinHttpOpenRequest envoie un verbe standard en majuscules (l'outil
# de déploiement d'Office passe « Get », refusé par sa passerelle).
f = (runtime_dlls / "winhttp.rs").read_text(encoding="utf-8")
ok("fn verbe_standard_en_majuscules_2730" in f and "fn standard_verb(" in f,
   "WinHTTP : verbe standard envoyé en majuscules (#2730)")
