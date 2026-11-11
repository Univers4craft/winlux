# #1609 : le journal Direct2D décrit le DC où une image est présentée.
step "Direct2D : journal du DC de présentation (#1609)" cargo test -q -p winlux -- journal_d2d_decrit_un_dc_memoire
