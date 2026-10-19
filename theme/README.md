# Thème Winlux

Cette première base fournit une palette et des composants visuels inspirés de
Windows 11, sans reprendre de fichiers Microsoft. Elle est destinée au futur
environnement graphique de l'ISO ; le runtime lui-même reste indépendant du
serveur graphique.

## État et travaux restants

Le fichier CSS n'est pas encore branché à une application GTK/Qt ni à un shell
de bureau. Il ne crée pas de fenêtres et ne change pas l'apparence des
applications Windows exécutées par Winlux.

- Choisir et construire un shell graphique Linux libre.
- Créer barre des tâches, lanceur/menu, explorateur et centre de notifications.
- Relier le shell au runtime : lancement, liste des fenêtres, focus, fermeture,
  presse-papiers et chemins Windows/Linux.
- Ajouter les variantes clair/sombre, le clavier, l'accessibilité et les tests
  sur plusieurs résolutions.
- Intégrer ces composants à une image ISO Linux réellement amorçable.
