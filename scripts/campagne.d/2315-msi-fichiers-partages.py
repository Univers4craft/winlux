# #2315 : retirer un produit MSI ne supprime pas un fichier qu'un autre
# produit installé a posé au même chemin (mfc140u.dll x64 disparue).
src = (ROOT / "runtime/src/msi/install.rs").read_text(encoding="utf-8")
ok("files_of_other_products(&m.product)" in src
   and "shared.contains(&path_key(f))" in src,
   "msiexec : fichiers d'un autre produit installé conservés à la désinstallation (#2315)")
