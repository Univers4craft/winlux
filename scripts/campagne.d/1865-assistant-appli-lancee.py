# #1865 : l'appli lancée sans attente en fin d'installation ne garde pas la sortie lue par l'assistant.
r = (runtime_dlls.parent / "setup" / "run.rs").read_text(encoding="utf-8")
# Noms : spawn_with et Stdio::null dans son corps (#2711).
_spawn = r[r.index("fn spawn_with("):]
_spawn = _spawn[:_spawn.index("\n}\n")]
ok("Programme lancé sans attendre" in _spawn and "Stdio::null" in _spawn,
   "programme lancé sans attente : sortie de l'installation non héritée (#1865)")
n = (runtime_dlls / "ntdll_file.rs").read_text(encoding="utf-8")
# Le comportement est vérifié par les tests nommés (renommage puis
# effacement à la fermeture, rm-* de Discord), plus par un fragment de trace
# (#2509 a déplacé l'effacement à la fermeture).
ok("fn dossier_renomme_puis_efface_comme_remove_dir_all" in n
   and "fn ramasse_miettes_rm_n_de_discord" in n
   and "fn efface_a_la_fermeture_apres_renommage" in n,
   "NtSetInformationFile : renommages et suppressions des fichiers rm-* de Discord, testés")
