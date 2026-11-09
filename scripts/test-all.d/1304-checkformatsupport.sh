# #1304 : CheckFormatSupport déclare les capacités réelles de chaque format
# (ANGLE, Skia : processus GPU de Claude pour Windows, #996).
step "CheckFormatSupport par format (#1304)" cargo test -q -p winlux -- formats_de_rendu_annonces formats_annonces_ont_une_traduction_opengl
