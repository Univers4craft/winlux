# Signatures Authenticode (#1387) : certificat expiré mais signature
# horodatée pendant sa validité (jeton RFC 3161), comme Steam.
step "Authenticode : signature horodatée au certificat expiré (#1387)" cargo test -q -p winlux -- signature_horodatee_au_certificat_expire
