# #1885 : effacement à la fermeture sous le nom actuel, ouvertures relatives et listes de dossier.
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
# Nom du test dossier_renomme_puis_efface_comme_remove_dir_all (#2711).
ok("effacement à la fermeture de {}" in k
   and "fn dossier_renomme_puis_efface_comme_remove_dir_all" in (runtime_dlls / "ntdll_file.rs").read_text(encoding="utf-8"),
   "CloseHandle : FILE_FLAG_DELETE_ON_CLOSE efface le fichier ou dossier renommé (#1885)")
# Noms des classes servies : FileIdBothDirectoryInfo et FileFullDirectoryInfo,
# variantes Restart comprises (#2711).
ok("FileIdBothDirectory(Restart)Info" in k and "FileFullDirectory(Restart)Info" in k,
   "GetFileInformationByHandleEx : listes de dossier servies (#1885)")
n = (runtime_dlls / "ntdll.rs").read_text(encoding="utf-8")
ok("options & FILE_DELETE_ON_CLOSE != 0" in n and "OBJECT_ATTRIBUTES.RootDirectory" in n,
   "NtCreateFile : FILE_DELETE_ON_CLOSE et RootDirectory honorés (#1885)")
