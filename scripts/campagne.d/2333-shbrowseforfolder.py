# #2333 : SHBrowseForFolderA/W ouvrent la boîte de choix de dossier
# (l'Explorateur en mode dossier) au lieu de rendre NULL sans rien afficher ;
# BROWSEINFO convertie pour les programmes x86 (outil de déploiement d'Office).
apps = (ROOT / "runtime" / "src" / "dlls" / "apps_win32.rs").read_text(encoding="utf-8")
jeux = (ROOT / "runtime" / "src" / "dlls" / "games_win32.rs").read_text(encoding="utf-8")
dlg = (ROOT / "runtime" / "src" / "wow32" / "comdlg.rs").read_text(encoding="utf-8")
ok('"SHBrowseForFolderW" => fn sh_browse_for_folder_w' in apps
   and '"SHBrowseForFolderW" => fn null_ptr' not in jeux,
   "SHBrowseForFolderW ouvre la boîte de choix de dossier (#2333)")
ok("static BROWSEINFO: Layout" in dlg, "BROWSEINFO convertie pour les programmes x86 (#2333)")
