# #1199 : stylet (pression, inclinaison) pour Photoshop : WM_POINTER et wintab32.
# Vérifié sur une vraie tablette par damien (fiche Photoshop, phase B).
ok('"GetPointerPenInfo" => fn get_pointer_pen_info' in (runtime_dlls / "pointer.rs").read_text(encoding="utf-8")
   and 'name: "wintab32"' in (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
   and "stylet_en_messages_pointeur_avec_pression" in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8"),
   "Stylet : WM_POINTER avec pression et inclinaison, wintab32.dll maison (#1199)")
