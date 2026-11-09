# `winlux --version` : version et commit de construction (#1266).
step "winlux --version avec le commit de construction (#1266)" cargo test -q -p winlux -- version_affiche_le_commit_de_construction
