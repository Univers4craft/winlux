# Premier clic sur « Non » perdu (#1286) : l'écho X d'une remise du focus
# au bouton par défaut ne rend plus le focus Win32 à celui-ci.
step "FocusIn périmé d'un contrôle ignoré (#1286)" cargo test -q -p winlux -- focus_perime_d_un_controle_ignore
