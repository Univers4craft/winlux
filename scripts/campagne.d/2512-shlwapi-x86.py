# #2512 : toute fonction que shlwapi sert en x64 a une passerelle x86
# (PathIsUNCW tuait l'enfant de l'outil de déploiement d'Office).
w = (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8")
c = (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8")
ok("fn shlwapi_toujours_servie_en_x86" in w and "#2512" in c,
   "shlwapi : chaque export servi en x86, dont PathIsUNCW (#2512)")
