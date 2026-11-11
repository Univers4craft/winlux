# #1573 : cible WIC en alpha seul 8 bits et en RGBA prémultiplié (Paint.NET).
step "Direct2D : cible WIC en alpha seul et RGBA (#1573)" cargo test -q -p winlux -- cible_wic_rgba_et_alpha_seul
