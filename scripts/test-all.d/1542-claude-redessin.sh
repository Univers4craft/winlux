# #1542 : l'image DXGI couvre la taille réelle de la fenêtre après un
# agrandissement (Claude pour Windows : plus de noir ni de décalage).
if command -v xvfb-run >/dev/null 2>&1; then
    step "Image présentée sur toute la fenêtre redimensionnée (#1542)" xvfb-run -a cargo test -q -p winlux -- --ignored image_presentee_couvre_la_fenetre_redimensionnee_x11
fi
