# #2750 : installeur web (Paint.NET) au double-clic, l'assistant demande
# l'accord de téléchargement (paquet, éditeur, source, empreinte nommés,
# « Annuler » par défaut, cadre de focus visible) puis relance avec
# --allow-download ; un refus ne télécharge rien.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Assistant : accord de téléchargement d'un installeur web, sous Xvfb (#2750)" env NO_AT_BRIDGE=1 xvfb-run -a python3 -B scripts/test-setup-wizard-telechargement.py
else
    skip "Assistant : accord de téléchargement d'un installeur web, sous Xvfb (#2750)" "xvfb-run absent"
fi
