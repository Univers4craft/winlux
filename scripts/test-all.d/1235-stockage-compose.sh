# Stockage composé (#1235) : docfile [MS-CFB] écrit puis relu, reconnu par
# StgIsStorageFile ; ILockBytes sur HGLOBAL et mode transactionnel ;
# données corrompues refusées sans panique.
step "Stockage composé : docfile écrit, relu et reconnu (#1235)" cargo test -q -p winlux -- docfile_ecrit_puis_relu_et_reconnu docfile_en_memoire_et_transaction format_cfb_aller_retour_et_donnees_corrompues
