# #2814 : System.Net.HttpWebRequest sur notre WinHTTP, témoin Reseau contre
# un serveur HTTP local (GET, reprise AddRange, 404, mandataire).
if [ "$quick" = 0 ]; then
    step "System.Net : HttpWebRequest sur WinHTTP, reprise, 404 et mandataire (#2814)" sh scripts/test-webrequest.sh
else
    skip "System.Net : HttpWebRequest sur WinHTTP, reprise, 404 et mandataire (#2814)" "mode rapide"
fi
