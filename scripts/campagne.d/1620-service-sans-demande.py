# Un service Windows sans fenêtre n'ouvre pas la demande d'autorisation (refus visible dans Paramètres) (#1620).
perm = (ROOT / "runtime/src/permissions.rs").read_text(encoding="utf-8")
ok("fn is_service()" in perm and "fn un_service_ne_demande_pas" in perm,
   "Autorisations : un service sans fenêtre ne pose pas de question à l'ouverture de session (#1620)")
