# #1651 : DrawImage avec rectangle source place son coin haut gauche au décalage.
step "Direct2D : DrawImage place le rectangle source au décalage (#1651)" cargo test -q -p winlux -- effets_flood_crop_premultiply
