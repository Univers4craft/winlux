# Bac à sable de Chromium (#767) : ntdll exporte toutes les fonctions qu'il
# intercepte dans chaque enfant (NtOpenThread, NtOpenProcess, jetons…).
step "ntdll : fonctions interceptées par le bac à sable (#767)" cargo test -q -p winlux -- interceptions_de_base_du_bac_a_sable
