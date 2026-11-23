# #3170 : MsgWaitForMultipleObjects se réveillait toutes les 10 ms, même en INFINITE.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Attente : MsgWaitForMultipleObjects dort jusqu'au signal, sous Xvfb (#3170)" xvfb-run -a -s '-screen 0 1024x768x24' sh scripts/test-msgwait-repos.sh
else
    skip "Attente : MsgWaitForMultipleObjects dort jusqu'au signal, sous Xvfb (#3170)" "xvfb-run absent"
fi
