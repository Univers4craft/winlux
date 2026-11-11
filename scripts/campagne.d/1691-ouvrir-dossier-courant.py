# #1691 : GetOpenFileName change le dossier courant, sauf OFN_NOCHANGEDIR.
c = (runtime_dlls / "comdlg32_files.rs").read_text(encoding="utf-8")
ok("flags & OFN_NOCHANGEDIR == 0" in c and "set_current_directory_w" in c,
   "GetOpenFileName : dossier courant = dossier du fichier choisi (#1691)")
