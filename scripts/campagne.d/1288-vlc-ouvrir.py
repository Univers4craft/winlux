# VLC (Qt) : la boîte d'ouverture réveille la file du fil quand l'Explorateur se ferme.
ok("gui::post_to_thread(tid, gui::Msg::new(0, 0, 0, 0))" in (runtime_dlls / "comdlg32_files.rs").read_text(encoding="utf-8")
   and "boucle_imbriquee_reveillee_a_la_fin_de_l_explorateur" in (SCRIPTS / "test-all.d" / "1288-vlc-ouvrir.sh").read_text(encoding="utf-8"),
   "VLC ne se fige plus après Média > Ouvrir un fichier (#1288)")
