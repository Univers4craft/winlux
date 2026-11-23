# Un dossier de plus de 1 024 éléments s'affiche en entier dans l'Explorateur :
# le stockage des volets grandit à la demande au lieu de 1 024 places fixes
# (#3156). Le comportement est vérifié par explorer_regression.exe
# (big_folder_checks, dossier réel de 3 000 fichiers).
explorer_3156 = (ROOT / "testexe/src/shell/explorer.rs").read_text(encoding="utf-8")
print("#3156 : big_folder_checks appelé par regression_checks :",
      "    big_folder_checks(root);" in explorer_3156)
ok("unsafe fn big_folder_checks(root: &[u8])" in explorer_3156 and "    big_folder_checks(root);" in explorer_3156
   and "un dossier de 3 000 fichiers s'affiche en entier, trie (#3156)" in explorer_3156,
   "Explorateur : un dossier de 3 000 fichiers s'affiche en entier (#3156)")
