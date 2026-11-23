# #3353 : SetWindowPos(SWP_FRAMECHANGED) n'envoyait pas WM_NCCALCSIZE à un programme 32 bits : son cadre maison gardait celui du gestionnaire.
step "cadre maison d'un programme x86 : WM_NCCALCSIZE et NCCALCSIZE_PARAMS x86 (#3353)" sh scripts/test-cadre-maison32.sh
