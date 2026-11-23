# Chromium 160 : barre de titre d'openbox au-dessus de la sienne (#3205).
# CreateWindowEx envoie WM_NCCALCSIZE entre WM_NCCREATE et WM_CREATE ; le
# test unitaire imite Chromium, qui laisse ce premier WM_NCCALCSIZE à
# DefWindowProc puis garde tout le rectangle sur SWP_FRAMECHANGED.
_user32 = (ROOT / "runtime" / "src" / "dlls" / "user32.rs").read_text(encoding="utf-8")
_wow = (ROOT / "runtime" / "src" / "wow32" / "user.rs").read_text(encoding="utf-8")
ok("fn creation_envoie_wm_nccalcsize_avant_wm_create()" in _user32
   and "fn proc_chromium_3205(" in _user32 and "(#3205)" in _wow,
   "Chromium : WM_NCCALCSIZE à la création, cadre maison reconnu sur SWP_FRAMECHANGED (#3205)")
