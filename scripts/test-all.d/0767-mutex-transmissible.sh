# Bac à sable de Chromium (#767) : le mutex « vivant » du courtier se copie
# dans chaque cible ; possession, libération et abandon vus des deux côtés.
step "DuplicateHandle : mutex et sémaphore transmissibles (#767)" cargo test -q -p winlux -- mutex_et_semaphore_transmissibles
