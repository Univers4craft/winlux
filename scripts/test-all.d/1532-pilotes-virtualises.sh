# #1532 : installation virtualisée des pilotes constructeurs (.inf, services pilotes, notification).
step "Pilotes virtualisés : lecture des .inf, notification en 4 langues (#1532)" cargo test -q -p winlux -- drivers::
