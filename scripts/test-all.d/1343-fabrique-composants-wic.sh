# #1343 : la fabrique WIC répond à IWICComponentFactory et crée des
# contextes de couleur (Paint.NET, ImagingFactory et ColorContexts).
step "Fabrique de composants WIC et contextes de couleur (#1343)" cargo test -q -p winlux -- fabrique_de_composants_et_contextes_de_couleur
