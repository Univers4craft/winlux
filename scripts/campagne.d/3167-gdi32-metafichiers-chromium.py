# #3167 : gdi32 exporte EnumEnhMetaFile, GdiComment et PlayEnhMetaFileRecord,
# que Chromium 160 charge en différé et sans lesquelles il s'arrête au
# démarrage. Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "gdi32_more.rs").read_text(encoding="utf-8")
ok("fn metafichiers_ameliores_charges_en_differe_par_chromium_3167" in t, "gdi32 sert les trois fonctions de métafichier que Chromium charge en différé (#3167)")
