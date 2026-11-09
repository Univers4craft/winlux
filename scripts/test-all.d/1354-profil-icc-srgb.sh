# #1354 : Paint.NET hache le profil sRGB de Direct2D et refuse moins de
# 128 octets.
step "Profil ICC sRGB du contexte de couleur Direct2D (#1354)" cargo test -q -p winlux -- profils_srgb_et_scrgb_complets contexte_srgb_rend_un_profil_icc
