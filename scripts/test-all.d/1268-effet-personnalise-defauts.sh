# #1268 : les valeurs par défaut des propriétés liées d'un effet Direct2D
# personnalisé passent par leur setter après Initialize (Paint.NET).
step "Effet personnalisé : défauts après Initialize (#1268)" cargo test -q -p winlux -- valeurs_par_defaut_posees_apres_initialize
