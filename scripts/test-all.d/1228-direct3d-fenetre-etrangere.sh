# #1228 partie 2 : Direct3D présente dans la fenêtre X11 d'un autre processus
# (processus GPU de Chromium, ANGLE sur Direct3D 11).
step "Direct3D dans la fenêtre d'un autre processus (#1228)" cargo test -q -p winlux -- direct3d_trouve_la_fenetre_x11_d_un_autre_processus
