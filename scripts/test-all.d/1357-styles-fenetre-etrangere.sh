# #1357 : le processus GPU de Chromium lit WS_EX_LAYERED sur la fenêtre
# surgissante du navigateur ; sans lui, la marge d'ombre sortait noire.
step "Styles des fenêtres d'un autre processus (#1357)" cargo test -q -p winlux -- styles_de_la_fenetre_d_un_autre_processus
