# #1297 : un effet Direct2D répond à ID2D1Image (DrawImage de Paint.NET),
# et les refus de QueryInterface répétés ne noient plus le journal.
step "Effet Direct2D en ID2D1Image (#1297)" cargo test -q -p winlux -- effet_rend_sa_fabrique_par_id2d1resource refus_d_interface_notes_avec_parcimonie
