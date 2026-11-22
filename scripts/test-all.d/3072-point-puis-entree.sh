# #3072 : le caractère tapé juste avant Entrée arrivait après elle (Notepad++).
if command -v xvfb-run >/dev/null 2>&1; then
    step "Clavier : « . » puis Entrée déjà en file, le point arrive avant Entrée, sous Xvfb (#3072)" xvfb-run -a -s '-screen 0 1024x768x24' sh scripts/test-point-puis-entree.sh
else
    skip "Clavier : « . » puis Entrée déjà en file, le point arrive avant Entrée, sous Xvfb (#3072)" "xvfb-run absent"
fi
