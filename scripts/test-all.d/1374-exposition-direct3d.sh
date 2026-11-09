# #1374 : une fenêtre où Direct3D présente (processus GPU de Chromium) n'est
# pas repeinte en gris à l'exposition ; le programme reçoit WM_PAINT.
if [ "$quick" != 1 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Marque Direct3D lue par le propriétaire de la fenêtre (#1374)" xvfb-run -a cargo test -q -p winlux -- --ignored marque_direct3d_lue_par_le_proprietaire_x11
else
    skip "Marque Direct3D lue par le propriétaire de la fenêtre (#1374)" "xvfb-run absent ou mode rapide"
fi
