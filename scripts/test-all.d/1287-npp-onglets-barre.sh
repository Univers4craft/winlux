# Notepad++ : icône d'onglet à gauche du libellé (IMAGEINFO.rcImage à
# l'offset 24) et boutons de la barre d'outils alignés sur leurs images
# (TB_ADDBITMAP avec un HBITMAP nul ne consomme pas d'index) (#1287).
step "IMAGEINFO rcImage à l'offset 24 (#1287)" cargo test -q -p winlux -- image_info_place_rc_image_a_l_offset_24
step "TB_ADDBITMAP nul sans index (#1287)" cargo test -q -p winlux -- tb_addbitmap_nul_ne_consomme_pas_d_index
