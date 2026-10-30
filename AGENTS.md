# Règles de travail pour les agents

## SUIVI DES TÂCHES (OBLIGATOIRE)

**Source unique de vérité : [Suivi.md](Suivi.md)**

- **Avant d'agir** : Consulte `Suivi.md`. Prends une tâche non cochée dans "À FAIRE" et commence à la réaliser. **Coche-la `[x]` IMMÉDIATEMENT** en la déplaçant vers "DÉJÀ FAIT" ou en la marquant faite.
- **Pendant l'action** : Travaille uniquement sur cette tâche. Si tu découvres une sous-tâche nécessaire, ajoute-la à `Suivi.md` dans la bonne section.
- **Après avoir terminé** : Teste (`cargo test -p winlux`, `scripts/build-tests.sh`, `scripts/test-runtime.sh` si pertinent). **Met à jour `Suivi.md`** pour marquer la tâche comme `[x]` avec contexte si besoin. Commit et push.
- **À jour en temps réel** : À CHAQUE action concrète (ajout, correction, refactor), `Suivi.md` DOIT être mis à jour dans le même commit ou juste après. Pas "plus tard".
- **Miroirs** : `TODO.md` et `TRACKING.md` sont des miroirs. `Suivi.md` est la source à consulter et à mettre à jour en priorité.

## Efficacité

- Comprendre la demande et son périmètre avant d'agir. Pour une question ou une
  demande de plan, répondre sans modifier le dépôt ; pour une demande de
  changement, implémenter le changement demandé sans élargir le périmètre.
- Explorer de façon ciblée : repérer les fichiers utiles, lire le contexte
  nécessaire une fois, puis agir. Ne pas relancer des recherches ou relire les
  mêmes fichiers sans nouvel élément qui le justifie.
- Grouper les recherches et lectures indépendantes dans un même appel. Éviter
  les inspections globales, les sous-agents et les commandes coûteuses quand
  une vérification ciblée suffit.
- Choisir la plus petite validation qui couvre le changement. Ne pas lancer
  systématiquement toute la suite, une compilation complète ou des vérifications
  répétées ; élargir seulement si les résultats ou le risque le justifient.
- Ne pas multiplier les messages de progression : communiquer seulement lors
  d'un changement important de phase ou en cas de blocage. Garder la réponse
  finale concise et signaler les vérifications réellement effectuées.

## Exactitude et modifications

- Vérifier le code et la documentation pertinents avant d'affirmer qu'une
  fonctionnalité existe ou manque ; distinguer clairement l'état actuel des
  objectifs proposés.
- Effectuer des changements précis, cohérents avec les conventions du dépôt.
  Ne pas écraser ni annuler les modifications préexistantes de l'utilisateur.
- Mettre à jour la documentation directement liée au changement. Ne pas
  modifier de fichiers sans rapport avec la demande.
- Ne pas masquer les erreurs de validation. Si une commande échoue, examiner
  son résultat et ne relancer que si une action corrective ou une autre
  vérification est nécessaire.
