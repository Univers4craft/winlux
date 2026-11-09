# Bac à sable de Chromium (#1352) : TokenPrivileges assez long,
# AddMandatoryAce, et ProcessHandleTable vide sur le processus courant.
step "Jeton et ACE d'étiquette pour un renderer en bac à sable (#1352)" cargo test -q -p winlux -- privileges_du_jeton_et_ace_obligatoire
step "ProcessHandleTable du processus courant (#1352)" cargo test -q -p winlux -- table_des_handles_du_processus_courant_vide
