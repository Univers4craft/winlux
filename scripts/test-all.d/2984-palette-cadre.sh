# #2984, #2983 : palette flottante sous openbox, cadre du gestionnaire
# compris dans son rectangle Win32 (vraie fenêtre sous Xvfb).
if command -v xvfb-run >/dev/null 2>&1 && command -v openbox >/dev/null 2>&1 && command -v xwininfo >/dev/null 2>&1 && command -v xprop >/dev/null 2>&1; then
    step "Palette flottante : cadre du gestionnaire dans son rectangle, sous openbox (#2984)" xvfb-run -a -s '-screen 0 1280x800x24' sh scripts/test-palette-cadre.sh
else
    skip "Palette flottante : cadre du gestionnaire dans son rectangle, sous openbox (#2984)" "xvfb-run, openbox, xwininfo ou xprop absent"
fi
