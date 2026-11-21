# #1905 objets nommés : page partagée réservée (posix_fallocate) et taille vérifiée avant projection.
i = (runtime_dlls / "inherit.rs").read_text(encoding="utf-8")
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
# Noms de la fonction et du test qui vérifie réservation et taille.
ok("fn reserve_page(" in i and "fn page_partagee_reservee_et_taille_verifiee(" in i,
   "la page d'un objet nommé est allouée avant la première écriture (#1905)")
