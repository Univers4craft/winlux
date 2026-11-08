# Faute matérielle dans un module tiers (#996, ANGLE de Chromium) : le
# journal de trace donne les registres et les octets de code autour de rip.
step "Registres d'une faute matérielle au journal (#996)" cargo test -q -p winlux -- registres_d_une_faute_pour_le_journal
