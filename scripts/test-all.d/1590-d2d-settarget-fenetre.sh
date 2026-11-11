# #1590 : une cible HDC dessine dans un bitmap choisi par SetTarget, puis retrouve ses pixels.
step "Direct2D : SetTarget vers un bitmap depuis une cible HDC (#1590)" cargo test -q -p winlux -- cible_hdc_dessine_dans_un_bitmap_puis_revient
