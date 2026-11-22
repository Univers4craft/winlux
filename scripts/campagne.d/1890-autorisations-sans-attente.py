# Une demande d'autorisation ne fait jamais attendre l'application : refus
# immédiat, fenêtre ouverte à côté, une seule par capacité ; un blocage fait
# dans Paramètres s'applique sans relancer l'application (#1890).
_p = (ROOT / "runtime" / "src" / "permissions.rs").read_text(encoding="utf-8")
ok("fn allowed_for(" in _p and "struct Session" in _p,
   "autorisations : réponse immédiate, choix relus quand le fichier change (#1890)")
ok(all(f"fn {t}" in _p for t in ("la_question_ne_fait_pas_attendre_1890",
                                  "autorise_dans_la_fenetre_vaut_pour_l_appel_suivant_1890",
                                  "blocage_dans_parametres_sans_relancer_1890")),
   "tests : pas d'attente, réponse vue à l'appel suivant, blocage sans relancer (#1890)")
