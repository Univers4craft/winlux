# #3170 : l'Explorateur ouvert au repos dort jusqu'au prochain message ou tic,
# au lieu de se réveiller ~100 fois par seconde.
if command -v xvfb-run >/dev/null 2>&1 && test -x target/release/winlux && test -f target/x86_64-pc-windows-msvc/release/gui_demo.exe; then
    step "Explorateur au repos : moins de 20 réveils par seconde (#3170)" xvfb-run -a -s "-screen 0 1366x768x24" /usr/bin/python3 -B scripts/test-explorateur-repos-xvfb.py
else
    skip "Explorateur au repos : moins de 20 réveils par seconde (#3170)" "xvfb-run, runtime ou gui_demo.exe absent"
fi
