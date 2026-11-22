# #2936 : une boîte « Ouvrir » ou « Enregistrer sous » de style Explorateur
# avec OFN_ENABLEHOOK appelle son crochet : WM_INITDIALOG, CDN_INITDONE,
# puis CDN_FILEOK avec le nom choisi dans lpstrFile (Learn, « CDN_FILEOK
# notification code », « OFNHookProc »). Sans CDN_FILEOK, Paint.NET gardait
# le nom proposé et enregistrait « Sans titre » dans son dossier.
# Comportement gardé par le test unitaire
# dlls::comdlg32_files::tests::crochet_explorateur_recoit_cdn_fileok.
f_2936 = (ROOT / "runtime" / "src" / "dlls" / "comdlg32_files.rs").read_text(encoding="utf-8")
ok("struct FileHook" in f_2936 and "fn crochet_explorateur_recoit_cdn_fileok" in f_2936,
   "Boîtes de fichiers : le crochet reçoit CDN_FILEOK avec le nom choisi (#2936)")
