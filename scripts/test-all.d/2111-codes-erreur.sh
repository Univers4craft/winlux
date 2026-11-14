# #2111 et suivantes : codes d'erreur Win32 conformes à la documentation
# (succès sans code d'un appel précédent, échec avec le bon code).
step "Codes d'erreur Win32 conformes (#2111)" cargo test -q -p winlux -- conformite_codes_erreur
