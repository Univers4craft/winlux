# ApplicationDataContainer.Containers liste les sous-conteneurs (#2962).
c = (ROOT / "runtime" / "src" / "dlls" / "storage_settings.rs").read_text(encoding="utf-8")
ok("fn sous_conteneurs_listes_par_containers_2962" in c, "ApplicationDataContainer.Containers liste les sous-conteneurs (#2962)")
