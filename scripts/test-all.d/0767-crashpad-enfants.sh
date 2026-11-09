# Bac à sable de Chromium (#767) : le gestionnaire de crashpad reçu du
# parent est désigné par l'enfant ; rapport des limites des threads.
step "crashpad : gestionnaire désigné par les enfants (#767)" cargo test -q -p winlux -- gestionnaire_de_crashpad_designe_par_l_enfant
