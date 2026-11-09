# #1307, #1308 : message de plantage accentué, stop code APPLICATION_FAILURE
# pour une faute en espace utilisateur, [rsp] et cadres sur stderr.
step "Page de plantage d'une application (#1307, #1308)" cargo test -q -p winlux -- plantage_d_application_bien_nomme
