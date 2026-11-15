# #2574 : WinHttpCrackUrl lit et écrit l'URL_COMPONENTS d'un programme x86 à
# sa disposition de 60 octets (l'outil de déploiement d'Office lisait un
# pointeur nul).
g = (ROOT / "runtime/src/dlls/games_win32.rs").read_text(encoding="utf-8")
c = (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8")
ok("UrlComponents::load" in g and "#2574" in c,
   "winhttp : URL_COMPONENTS de WinHttpCrackUrl lu à la disposition x86 (#2574)")
