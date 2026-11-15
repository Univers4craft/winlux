# #2509 : FileDispositionInfo marque le fichier, effacé à la fermeture du
# handle ; DeleteFile = FALSE retire la marque (UnbufferedFileWriter de
# Chromium, Google Update dans l'installeur de Google Drive).
d = (ROOT / "runtime/src/dlls/doomed.rs").read_text(encoding="utf-8")
n = (ROOT / "runtime/src/dlls/ntdll_file.rs").read_text(encoding="utf-8")
ok("fn mark" in d and "fn unmark" in d
   and "fn marque_d_effacement_retiree_avant_fermeture" in n
   and "fn delete_on_close_de_creation_garde_l_effacement" in n,
   "marque d'effacement posée et retirée avant la fermeture (#2509)")
