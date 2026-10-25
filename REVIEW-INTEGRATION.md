# Revue des PR #1, #2 et #3 — 25 octobre 2026

Une CI verte ne démontre pas la compatibilité générale des logiciels Windows.
Les ajouts ci-dessous sont intégrés progressivement, avec tests de régression.
Les versions initiales restent consultables dans les commits des PR.

## PR #2 — runtime Win32

Retenus : `fgetpos`/`fsetpos`, recherche du module par adresse,
ouverture des événements et mutex nommés, registre des fabriques COM multi-usages.
Corrections : comptage des ouvertures nommées, cookies COM stables après révocation,
durée de vie des fabriques et appels COM réentrants hors verrou.

Reportés après revue :

- SEH : capture des registres après prologue Rust, déroulement incomplet,
  absence de restauration effective du contexte et parcours RBP non sécurisé.
- Déchargement DLL : absence de suivi des dépendances et des utilisateurs actifs.
- Propagation TLS : écritures non synchronisées dans les autres threads.
- Locale CRT : `localeconv` renvoyait des octets au lieu des pointeurs de `lconv`.
- `_snscanf` : formats bornés et modificateurs de taille non gérés.
- Services : gestionnaire enregistré mais jamais appelé, état non transmis au SCM.
- Duplication/héritage des handles et attributs de processus : succès annoncé
  sans application complète des options, fermeture ou héritage effectifs.
- Page de garde : détection fondée sur la proximité de RSP, pas sur les limites
  réelles de pile ; installation de la page proposée non raccordée au démarrage.

Ces fonctions ne sont pas présentées comme implémentées par cette intégration.

## Graphismes et interface

Voir [la revue des PR #1 et #3](REVIEW-GRAPHICS.md) pour les changements DIB,
les icônes et les parties multimédia reportées. Le magasin reste représenté par
un sac (et non un bouclier) ; les dents de l'engrenage restent dans la zone du glyphe.

Cette intégration ne publie pas de nouvelle version ni d'ISO. Les tests de bureau
en VM et la préparation d'une version installable restent des étapes distinctes.
