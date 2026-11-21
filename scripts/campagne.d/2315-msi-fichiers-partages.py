# #2315 : retirer un produit MSI ne supprime pas un fichier qu'un autre
# produit installé a posé au même chemin (mfc140u.dll x64 disparue).
src = (ROOT / "runtime/src/msi/install.rs").read_text(encoding="utf-8")
ok("fn files_of_other_products(" in src and "fn path_key(" in src
   and "fn desinstaller_garde_les_fichiers_d_un_autre_produit(" in (ROOT / "runtime/src/msi/tests.rs").read_text(encoding="utf-8"),
   "msiexec : fichiers d'un autre produit installé conservés à la désinstallation (#2315)")
