# #3297 : la session garde le bureau sans lancer un « sleep 1 » par seconde.
step "Session au repos : garde du bureau sans sleep chaque seconde (#3297)" python3 -B scripts/test-session-attente.py
