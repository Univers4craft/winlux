# Une demande d'autorisation attend la réponse : « Autoriser » laisse
# l'application continuer, « Refuser » la restreint, une seule fenêtre par
# capacité ; un blocage fait dans Paramètres s'applique sans relancer
# l'application (#1890).
_p = (ROOT / "runtime" / "src" / "permissions.rs").read_text(encoding="utf-8")
ok("fn allowed_for(" in _p and "struct Session" in _p,
   "autorisations : choix relus quand le fichier change, une seule question, délai maximal (#1890)")
ok(all(f"fn {t}" in _p for t in ("autoriser_laisse_l_application_continuer_1890",
                                  "refuser_restreint_l_application_1890",
                                  "fenetre_fermee_une_seule_question_1890",
                                  "sans_reponse_refus_apres_le_delai_1890",
                                  "les_autres_fils_continuent_pendant_la_question_1890",
                                  "blocage_dans_parametres_sans_relancer_1890")),
   "tests : autoriser continue, refuser restreint, une question, blocage sans relancer (#1890)")
