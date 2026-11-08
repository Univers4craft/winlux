# #1179 : tas Direct3D 12 partagés par les ressources placées (alias et
# barrière d'alias), tampon de profondeur placé rattaché au dessin.
step "Tas et ressources placées D3D12 (#1179)" cargo test -q -p winlux -- ressources_placees_au_meme_endroit_partagent_leur_memoire barriere_d_alias_rend_l_image_au_tas profondeur_placee_
