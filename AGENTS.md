# Règles de travail pour les agents

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
