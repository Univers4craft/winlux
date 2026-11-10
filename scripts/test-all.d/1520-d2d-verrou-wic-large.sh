# #1520 : CreateSharedBitmap depuis un verrou WIC en demi-flottants (Paint.NET).
step "Direct2D : bitmap partagé depuis un verrou WIC large (#1520)" cargo test -q -p winlux -- verrou_demi_flottant_en_bgra
