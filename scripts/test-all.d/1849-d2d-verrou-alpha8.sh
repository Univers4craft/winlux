# #1849 : verrou WIC 8bppAlpha partagé par CreateSharedBitmap.
step "Direct2D : verrou WIC 8bppAlpha (#1849)" cargo test -q -p winlux -- verrou_wic_alpha_8_bits
