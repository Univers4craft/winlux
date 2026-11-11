# #1905 objets nommés : page partagée réservée (posix_fallocate) et taille vérifiée avant projection.
i = (runtime_dlls / "inherit.rs").read_text(encoding="utf-8")
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("libc::posix_fallocate(fd, 0, 4096)" in i and "st.st_size < 4096" in i and "inherit::reserve_page(" in k,
   "la page d'un objet nommé est allouée avant la première écriture (#1905)")
