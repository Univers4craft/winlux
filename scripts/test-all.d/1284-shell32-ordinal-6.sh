# shell32 ordinal 6 = SHDefExtractIconW (#1284) : importé ainsi par
# wxmsw313u_core, Audacity s'arrêtait en fermant une seconde fenêtre de projet.
step "shell32 ordinal 6 (#1284)" cargo test -q -p winlux -- shell32_ordinal_6_est_sh_def_extract_icon_w
