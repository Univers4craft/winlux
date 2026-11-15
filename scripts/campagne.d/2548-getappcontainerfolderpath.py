# #2548 : userenv!GetAppContainerFolderPath rend le dossier LocalAppData.
# Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "knownfolders.rs").read_text(encoding="utf-8")
ok("fn dossier_d_un_conteneur_est_local_app_data" in t, "GetAppContainerFolderPath servie par userenv (#2548)")
