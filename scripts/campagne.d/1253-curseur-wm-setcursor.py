# WM_SETCURSOR envoyé au retrait d'un message souris, curseur de classe
# appliqué par DefWindowProc ; barre en I sur le texte de Sumatra PDF.
ok("fn send_set_cursor" in (runtime_dlls / "user32.rs").read_text()
   and "fn class_cursor_of" in (runtime_dlls / "gui.rs").read_text()
   and "wm_setcursor_applique_le_curseur_de_classe" in (SCRIPTS / "test-all.d" / "1253-curseur-wm-setcursor.sh").read_text(),
   "WM_SETCURSOR et curseur de classe : barre en I sur le texte de Sumatra PDF (#1253)")
