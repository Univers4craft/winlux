# #2852 : dans Enregistrer sous, Ctrl+A ne sélectionnait pas le nom proposé et la frappe s'ajoutait à la fin.
if command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 && test -x target/release/winlux && test -f target/x86_64-pc-windows-msvc/release/gui_demo.exe; then
    step "Boîte de fichiers : nom proposé sélectionné, Ctrl+A et remplacement (#2852)" xvfb-run -a -s "-screen 0 1366x768x24" /usr/bin/python3 -B scripts/test-pick-selection.py
fi
