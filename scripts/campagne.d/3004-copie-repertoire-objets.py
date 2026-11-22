# #3004 : DuplicateHandle copie un répertoire d'objets ouvert par
# NtOpenDirectoryObject, dans le processus et vers un enfant : le bac à sable
# de Chromium (Acrobat) en donne une copie à son enfant. Le comportement est
# gardé par le test unitaire nommé.
t = (runtime_dlls / "object_directory.rs").read_text(encoding="utf-8")
ok("fn copie_d_un_repertoire_d_objets_3004" in t, "DuplicateHandle copie un répertoire d'objets, aussi vers un autre processus (#3004)")
