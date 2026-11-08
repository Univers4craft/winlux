# CreateFileW (#1252) : FILE_FLAG_DELETE_ON_CLOSE honoré, et CREATE_NEW sur un
# fichier existant rend ERROR_FILE_EXISTS (80) ; réglages de Sumatra PDF portable.
step "Fichier supprimé à la fermeture, CREATE_NEW sur un existant (#1252)" cargo test -q -p winlux -- fichier_supprime_a_la_fermeture_et_create_new_sur_existant
