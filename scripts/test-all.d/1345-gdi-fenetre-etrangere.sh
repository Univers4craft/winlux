# #1345 : le processus GPU de Chromium peint les surgissants du navigateur
# par GetDC sur leur HWND puis BitBlt ; l'image doit atteindre la fenêtre.
step "GDI sur la fenêtre d'un autre processus (#1345)" cargo test -q -p winlux -- dessin_gdi_sur_la_fenetre_d_un_autre_processus
if [ "$quick" != 1 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Image posée sur une fenêtre X étrangère (#1345)" xvfb-run -a cargo test -q -p winlux -- --ignored image_posee_sur_un_drawable_etranger_x11
else
    skip "Image posée sur une fenêtre X étrangère (#1345)" "xvfb-run absent ou mode rapide"
fi
