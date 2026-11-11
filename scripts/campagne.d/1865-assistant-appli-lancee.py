# #1865 : l'appli lancée sans attente en fin d'installation ne garde pas la sortie lue par l'assistant.
r = (runtime_dlls.parent / "setup" / "run.rs").read_text(encoding="utf-8")
ok("} else if !wait {\n        // Programme lancé sans attendre" in r and "cmd.stdout(Stdio::null()).stderr(Stdio::null());" in r,
   "programme lancé sans attente : sortie de l'installation non héritée (#1865)")
n = (runtime_dlls / "ntdll_file.rs").read_text(encoding="utf-8")
ok("NtSetInformationFile(classe {class}) : suppression de" in n,
   "NtSetInformationFile : renommages et suppressions tracés (fichiers rm-* de Discord)")
