# Bac à sable de Chromium (#767) : relais Nt* de ntdll au format Windows
# (int 2Eh repris par Winlux), image à adresse fixe, appel depuis une copie.
step "ntdll : relais de service au format Windows (#767)" cargo test -q -p winlux -- relais_de_service_au_format_windows
