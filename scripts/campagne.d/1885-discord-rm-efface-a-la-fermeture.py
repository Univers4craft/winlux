# #1885 : effacement à la fermeture sous le nom actuel, ouvertures relatives et listes de dossier.
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("effacement à la fermeture de {}" in k and "std::fs::remove_dir(&path)" in k,
   "CloseHandle : FILE_FLAG_DELETE_ON_CLOSE efface le fichier ou dossier renommé (#1885)")
ok("11 => Some((37, true))" in k and "15 => Some((2, true))" in k,
   "GetFileInformationByHandleEx : listes de dossier servies (#1885)")
n = (runtime_dlls / "ntdll.rs").read_text(encoding="utf-8")
ok("options & FILE_DELETE_ON_CLOSE != 0" in n and "OBJECT_ATTRIBUTES.RootDirectory" in n,
   "NtCreateFile : FILE_DELETE_ON_CLOSE et RootDirectory honorés (#1885)")
