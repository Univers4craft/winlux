# #3291 : la veille de l'écran ne lance plus winlux-update --busy toutes les
# 15 s ni ne relit tout /proc toutes les 3 s, bureau au repos.
step "Veille de l'écran au repos : aucun programme lancé, /proc non relu (#3291)" python3 -B scripts/test-veille-repos.py
