# #2597 : HttpOpenRequestA/W et WinHttpOpenRequest lisent le tableau des types
# acceptés à la taille des pointeurs du programme (4 octets en x86 ; l'outil
# de déploiement d'Office mourait dans HttpOpenRequestA).
h = (ROOT / "runtime/src/dlls/winhttp.rs").read_text(encoding="utf-8")
c = (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8")
ok("fn program_pointers" in h and "fn accept_types_wide" in h and "#2597" in c,
   "WinINet/WinHTTP : types acceptés lus à la taille des pointeurs x86 (#2597)")
