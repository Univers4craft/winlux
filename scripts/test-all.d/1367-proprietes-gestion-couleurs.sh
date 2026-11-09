# #1367 : Paint.NET lit les contextes de couleur par GetValueRef.
step "Direct2D : contextes de couleur des effets (#1367)" cargo test -q -p winlux -- gestion_des_couleurs_decrit_ses_contextes contexte_de_couleur_non_pose_vaut_un_pointeur_nul
