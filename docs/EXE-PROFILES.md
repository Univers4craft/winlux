# Lancement et réglages des .exe

Un double-clic dans Explorer transmet directement le fichier à Winlux : aucun
profil préalable n'est nécessaire. Cela ne garantit pas que toutes ses API
Windows soient implémentées.

Le clic droit conserve les actions habituelles et ajoute **Réglages de
l'exécutable** pour les fichiers `.exe` uniquement. Les options disponibles
sont celles réellement appliquées par le runtime : échelle de son interface
et activation des traces. Il n'existe pas encore de réglage universel pour la
qualité 3D, DirectX, la mémoire vidéo ou les dépendances Windows.

Les réglages sont appliqués au prochain lancement après **Enregistrer**.
**Valeurs par défaut** retire le profil sans toucher au programme.
Ils sont stockés à côté de l'exécutable, par exemple :

```ini
# Mon jeu.exe.winlux.ini
ui_scale=1.5
trace=1
```

Valeurs d'échelle acceptées : `auto`, `1`, `1.25`, `1.5`, `2`.
`trace` accepte `0` ou `1`. Les autres lignes sont ignorées, jamais exécutées.
La lecture est limitée à 4 Kio. Le dossier doit être accessible en écriture ;
la page signale l'échec sans modifier les permissions du programme.
Déplacer ou renommer l'exécutable nécessite de déplacer ou renommer son profil.

Sans terminal installé, un programme console est exécuté avec l'entrée fermée ;
sa sortie est conservée dans `~/.local/state/winlux/last-launch.log` (ou sous
`XDG_STATE_HOME`). Les programmes demandant une saisie interactive nécessitent
toujours un émulateur de terminal. Les erreurs des applications graphiques
sont signalées même si leur message ne commence pas par `winlux:`.
