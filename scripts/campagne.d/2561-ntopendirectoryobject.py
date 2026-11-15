# #2561 : ntdll!NtOpenDirectoryObject ouvre les répertoires d'objets d'une
# session et d'un AppContainer au lieu de rendre STATUS_NOT_SUPPORTED.
# Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "object_directory.rs").read_text(encoding="utf-8")
ok("fn repertoire_des_objets_d_un_conteneur" in t, "NtOpenDirectoryObject ouvre le répertoire d'un conteneur (#2561)")
