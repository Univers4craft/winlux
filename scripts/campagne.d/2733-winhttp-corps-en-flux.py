# #2733 : WinHTTP lit le corps d'une réponse au fil de son arrivée, sans
# plafond de taille (l'outil de déploiement d'Office télécharge un fichier de
# plus de 1 Gio).
f = (runtime_dlls / "winhttp.rs").read_text(encoding="utf-8")
ok("fn gros_corps_lu_au_fil_de_l_eau_2733" in f and "struct Transfer" in f,
   "WinHTTP : corps de réponse lu en flux, sans plafond de taille (#2733)")
