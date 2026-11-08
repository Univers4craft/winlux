# WM_SETCURSOR (#1253) : envoyé au retrait d'un message souris, DefWindowProc
# applique le curseur de classe (barre en I sur le texte de Sumatra PDF).
step "WM_SETCURSOR et curseur de classe (#1253)" cargo test -q -p winlux -- wm_setcursor_applique_le_curseur_de_classe
