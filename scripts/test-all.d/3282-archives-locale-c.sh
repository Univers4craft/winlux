# #3282 : sous une locale héritée C, un nom accentué traverse un 7z écrit
# puis relu par bsdtar, car chaque outil d'archive tourne sous C.UTF-8.
step "archives : nom accentué en 7z sous LC_ALL=C (#3282)" env LC_ALL=C LANG= cargo test -q -p winlux -- sept_z_nom_accentue_locale_heritee_3282
