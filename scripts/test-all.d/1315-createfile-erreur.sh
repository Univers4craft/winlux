# CreateFileW réussi (#1315) : GetLastError vaut 0, ou 183 pour
# CREATE_ALWAYS / OPEN_ALWAYS sur un fichier existant, jamais l'erreur d'avant.
step "CreateFileW réussi remet l'erreur à zéro (#1315)" cargo test -q -p winlux -- create_file_reussi_remet_l_erreur_a_zero
