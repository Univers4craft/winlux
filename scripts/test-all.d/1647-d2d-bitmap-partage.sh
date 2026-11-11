# #1647 : CreateSharedBitmap partage la mémoire du verrou WIC (tampon du pinceau de Paint.NET).
step "Direct2D : bitmap partagé relu à chaque usage (#1647)" cargo test -q -p winlux -- bitmap_partage_depuis_un_verrou_wic
