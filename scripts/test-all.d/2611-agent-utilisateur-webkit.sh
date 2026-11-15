# #2611 : put_UserAgent, WebKit ne refuse jamais la chaîne réécrite.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Agent utilisateur WebView2 accepté par WebKit, sous Xvfb (#2611)" env NO_AT_BRIDGE=1 xvfb-run -a python3 -B scripts/test-webview-agent.py
else
    step "Agent utilisateur WebView2 réécrit selon RFC 9110 (#2611)" python3 -B scripts/test-webview-agent.py
fi
