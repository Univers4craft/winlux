# #2870 : la lettre tapée après un raccourci pris par TranslateAccelerator (Ctrl+N de Notepad++) était perdue.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Clavier : Ctrl+N pris par un accélérateur, puis « nnn », aucune lettre perdue, sous Xvfb (#2870)" xvfb-run -a -s '-screen 0 1024x768x24' sh scripts/test-raccourci-puis-lettre.sh
else
    skip "Clavier : Ctrl+N pris par un accélérateur, puis « nnn », aucune lettre perdue, sous Xvfb (#2870)" "xvfb-run absent"
fi
