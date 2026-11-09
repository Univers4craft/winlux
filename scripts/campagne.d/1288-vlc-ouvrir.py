# VLC (Qt) : la boîte d'ouverture réveille la file du fil quand l'Explorateur se ferme.
ok("gui::post_to_thread(tid, gui::Msg::new(0, 0, 0, 0))" in (runtime_dlls / "comdlg32_files.rs").read_text(encoding="utf-8")
   and "boucle_imbriquee_reveillee_a_la_fin_de_l_explorateur" in (SCRIPTS / "test-all.d" / "1288-vlc-ouvrir.sh").read_text(encoding="utf-8"),
   "VLC ne se fige plus après Média > Ouvrir un fichier (#1288)")
# Qt 5 ne garde le choix qu'au moment d'IFileDialogEvents::OnFileOk.
ok("notify_file_ok(this)" in (runtime_dlls / "filedialog_com.rs").read_text(encoding="utf-8")
   and "boite_ouvrir_com_par_la_boite_integree" in (SCRIPTS / "test-all.d" / "1288-vlc-ouvrir.sh").read_text(encoding="utf-8"),
   "La boîte Ouvrir prévient ses récepteurs par OnFileOk, VLC lit le fichier choisi (#1288)")
