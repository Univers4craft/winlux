# #1523 : hid.dll complet pour les logiciels de périphériques (HidP_*, collections de tête, avis de branchement).
step "hid.dll : descripteur, HidP_*, collections de tête (#1523)" cargo test -q -p winlux -- hid_report devnotify
