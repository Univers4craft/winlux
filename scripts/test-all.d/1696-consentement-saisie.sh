# #1696 : verrou partagé entre la saisie simulée et les fenêtres de consentement.
step "fenêtres d'autorisation et d'élévation : winlux-permission et winlux-uac (#1696)" sh -c "python3 -B scripts/test-permission.py && python3 -B scripts/test-uac.py"
