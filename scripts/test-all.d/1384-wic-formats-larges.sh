# #1384 : Paint.NET crée un bitmap flottant pour tester la conversion de couleur.
step "WIC : formats larges et transformation de couleur (#1384)" cargo test -q -p winlux -- bitmap_flottant_et_transformation_de_couleur transformation_srgb_vers_scrgb demi_flottants_aller_retour pixel_flottant_premultiplie
