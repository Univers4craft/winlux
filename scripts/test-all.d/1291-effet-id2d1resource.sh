# #1291 : un effet Direct2D répond à ID2D1Resource et rend sa fabrique
# (Paint.NET relie ses objets à leur fabrique par GetFactory).
step "Effet Direct2D : ID2D1Resource et GetFactory (#1291)" cargo test -q -p winlux -- effet_rend_sa_fabrique_par_id2d1resource
