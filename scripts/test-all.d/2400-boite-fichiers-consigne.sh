# #2400 : la consigne du programme (lpszTitle, lpstrTitle) n'apparaissait pas dans la boîte de fichiers.
if command -v xvfb-run >/dev/null 2>&1 && test -x target/release/winlux && test -f target/x86_64-pc-windows-msvc/release/gui_demo.exe; then
    step "Boîte de fichiers : consigne du programme affichée (#2400)" xvfb-run -a -s "-screen 0 1366x768x24" /usr/bin/python3 -B scripts/test-pick-consigne.py
fi
