# #2639 : wininet exporte InternetGetCookieExW (l'outil de déploiement
# d'Office l'importe en différé et mourait sur 0xC06D007F).
l = (ROOT / "runtime/src/dlls/lot_e_divers.rs").read_text(encoding="utf-8")
c = (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8")
ok('"InternetGetCookieExW" => fn InternetGetCookieExW' in l and "#2639" in c,
   "wininet : InternetGetCookieExW servie, aussi en x86 (#2639)")
