# #2416 : Paramètres > Confidentialité, l'interrupteur caméra ou micro revient
# à l'état réel quand l'UAC est refusée ou que l'opération ne démarre pas.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Interrupteurs caméra et micro fidèles au réglage, sous Xvfb (#2416)" env NO_AT_BRIDGE=1 xvfb-run -a python3 -B scripts/test-settings-privacy.py
else
    skip "Interrupteurs caméra et micro fidèles au réglage, sous Xvfb (#2416)" "xvfb-run absent"
fi
