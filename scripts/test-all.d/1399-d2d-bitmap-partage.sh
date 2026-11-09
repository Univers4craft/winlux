# #1399 : Paint.NET peint le fond de ses contrôles par un bitmap partagé.
step "Direct2D : CreateSharedBitmap (#1399)" cargo test -q -p winlux -- bitmap_partage_depuis_un_verrou_wic
