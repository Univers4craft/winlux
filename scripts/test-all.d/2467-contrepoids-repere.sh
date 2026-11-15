# #2467 : barre centrée, le contrepoids mesure le bloc de droite depuis le
# repère 46, bouton interne de l'assistant compris ; une barre relancée ne
# reçoit plus le contrepoids calculé pour la précédente.
step "Contrepoids mesuré depuis le repère du bloc de droite (#2467)" python3 -B scripts/test-taskbar-balance-edge.py
