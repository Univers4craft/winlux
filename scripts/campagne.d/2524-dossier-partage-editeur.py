# #2524 : GetPublisherCacheFolder sert les dossiers déclarés dans le manifeste.
# Le comportement est gardé par les tests unitaires nommés.
sa = (runtime_dlls / "storage_appdata.rs").read_text(encoding="utf-8")
ok("fn dossier_partage_de_l_editeur_2524" in sa, "test de GetPublisherCacheFolder (#2524)")
mx = (ROOT / "runtime" / "src" / "setup" / "msix.rs").read_text(encoding="utf-8")
ok("fn dossiers_partages_de_l_editeur_2524" in mx, "test des PublisherCacheFolders du manifeste (#2524)")
