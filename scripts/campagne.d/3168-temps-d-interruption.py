# #3168 : kernel32 sert QueryInterruptTime et QueryInterruptTimePrecise, que
# Chromium 160 charge en différé (api-ms-win-core-realtime-l1-1-1) et sans
# lesquelles il s'arrête au démarrage. Gardé par le test unitaire nommé.
t = (runtime_dlls / "kernel32_releve.rs").read_text(encoding="utf-8")
ok("fn temps_d_interruption_veille_comprise_3168" in t, "kernel32 rend le temps d'interruption, veille comprise (#3168)")
