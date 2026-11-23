# #3432 : gui_demo.exe garde ses grands tableaux statiques en .bss, pas en .data.
if test -f target/x86_64-pc-windows-msvc/release/gui_demo.exe; then
    step "gui_demo.exe : moins de 2 Mio de données statiques initialisées (#3432)" /usr/bin/python3 -B scripts/test-gui-demo-data.py
else
    skip "gui_demo.exe : moins de 2 Mio de données statiques initialisées (#3432)" "gui_demo.exe absent"
fi
