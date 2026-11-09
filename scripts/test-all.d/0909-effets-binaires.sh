# D3DX9 (#909) : effets fx_2_0 déjà compilés (.fxo) lus comme les effets en
# texte (paramètres, échantillonneurs, techniques, passes, shaders).
step "D3DX9 : effets fx_2_0 binaires (#909)" cargo test -q -p winlux -- effet_fx_2_0_binaire_lu
