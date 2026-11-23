# #3394 : un programme 32 bits agrandi par le gestionnaire recevait WM_SIZE sans WM_WINDOWPOSCHANGED (rendu Chromium/CEF resté à l'ancienne taille).
step "programme x86 agrandi par openbox : WM_WINDOWPOSCHANGED avant WM_SIZE (#3394)" sh scripts/test-agrandie32.sh
