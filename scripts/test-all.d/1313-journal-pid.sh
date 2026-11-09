# WINLUX_LOG_FILE (#1313) : chaque ligne porte le programme et son pid, pour
# suivre dans un seul fichier les enfants GPU de Chromium lancés sans stderr.
step "WINLUX_LOG_FILE : programme et pid sur chaque ligne (#1313)" cargo test -q -p winlux -- journal_fichier_porte_le_programme_et_le_pid
