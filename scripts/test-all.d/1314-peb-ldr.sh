# PEB->Ldr (#1314) : le crashpad de Chromium lit les modules d'un processus
# planté dans les trois listes de PEB_LDR_DATA ; Ldr nul faisait échouer le dump.
step "PEB->Ldr et listes de modules (#1314)" cargo test -q -p winlux -- peb_ldr_liste_les_modules_dans_l_ordre_de_chargement
