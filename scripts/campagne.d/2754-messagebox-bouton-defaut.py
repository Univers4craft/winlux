# MessageBox : le bouton MB_DEFBUTTONn a le focus à l'ouverture, Entrée le
# choisit ; vérifié par une vraie boîte sous Xvfb (#2754).
_d = (ROOT / "runtime/src/dlls/dialogs.rs").read_text(encoding="utf-8")
ok("fn message_box_default_button(" in _d
   and (ROOT / "scripts/test-all.d/2754-messagebox-bouton-defaut.sh").exists()
   and (ROOT / "testc/boite/messagebox_defaut.c").exists(),
   "MessageBox : Entrée choisit le bouton par défaut, vérifié sous Xvfb (#2754)")
