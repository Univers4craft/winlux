# #1683 : SendInput, keybd_event et mouse_event tapent dans la session (XTEST), x64 et x86, sous autorisation.
step "saisie simulée : SendInput et keybd_event en x64 et x86, refus sans autorisation (#1683)" sh scripts/test-sendinput.sh
