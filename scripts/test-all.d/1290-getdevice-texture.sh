# #1290 : GetDevice d'une texture ou d'un tampon Direct3D 11 rend l'appareil
# (processus GPU de Claude pour Windows, #996).
step "GetDevice des textures Direct3D 11 (#1290)" cargo test -q -p winlux -- texture_et_tampon_rendent_leur_appareil
