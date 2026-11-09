# UAC (#1382) : signature Authenticode d'un .exe lue (table de sécurité du
# PE), condensat de l'image et signataire vérifiés.
step "UAC : éditeur lu dans la signature Authenticode (#1382)" cargo test -q -p winlux -- executable_signe_lu_et_verifie
