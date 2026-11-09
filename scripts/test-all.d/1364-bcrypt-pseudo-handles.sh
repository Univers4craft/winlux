# #1364 : SHA1.HashData de .NET hache par pseudo-handle CNG.
step "BCrypt : pseudo-handles d'algorithme (#1364)" cargo test -q -p winlux -- pseudo_handle_sha1_en_un_coup
