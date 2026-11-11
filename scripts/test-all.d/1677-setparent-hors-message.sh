# #1677 : SetParent sort une fenêtre (et ses enfants) du monde « message seulement ».
step "SetParent : fenêtre sortie de HWND_MESSAGE affichable (#1677)" cargo test -q -p winlux -- fenetre_message_seulement
