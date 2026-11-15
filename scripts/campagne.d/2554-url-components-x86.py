# #2554 : InternetCrackUrlA/W lit l'URL_COMPONENTS d'un programme x86 à sa
# disposition de 60 octets (l'outil de déploiement d'Office tombait en faute).
w = (ROOT / "runtime/src/dlls/wininet.rs").read_text(encoding="utf-8")
c = (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8")
ok("fn url_components_x86" in w and "fn from_x86" in w and "#2554" in c,
   "wininet : URL_COMPONENTS lu à la disposition x86 (#2554)")
