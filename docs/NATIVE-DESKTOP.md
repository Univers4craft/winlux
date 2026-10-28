# Session native Winlux

La session interactive associe Openbox (fenêtres), xfdesktop (bureau),
xfce4-panel (tâches et indicateurs) et Whisker (Démarrer). Les composants sont
préparés depuis les dépôts Ubuntu signés, dans un répertoire isolé : aucun
paquet n'est installé sur l'hôte par le script de préparation.

## Applications

- Explorer et Paramètres sont deux fenêtres PE indépendantes du runtime Winlux.
- Les applications Linux utilisent leurs propres fenêtres et contrôles. Edge
  démarre en fenêtre, avec son bac à sable conservé.
- Les fenêtres EWMH apparaissent dans le panneau, y compris celles du runtime.
- Mousepad est l'[éditeur maintenu par Xfce](https://docs.xfce.org/apps/mousepad/start).
  Les textes peuvent être ouverts, modifiés et sauvegardés ; xterm lance Bash.
- Whisker propose « Toutes les applications », une recherche, le défilement et
  les actions contextuelles natives. Les PE sont découverts dans les raccourcis
  Windows et le registre, ou enregistrés par `winlux-open` au premier lancement.
  Les icônes embarquées sont extraites quand elles existent. Un programme présent
  dans ce catalogue n'est pas nécessairement compatible avec le runtime.
- xfdesktop fournit le clic droit du bureau et la gestion des raccourcis. Les
  préférences sont initialisées seulement en leur absence, puis conservées.

## Veille et état système

XScreenSaver exécute son animation Pipes après dix minutes, avec un lanceur pour
la prévisualiser. La session demeure administrateur à ce stade : son verrouillage
par mot de passe est désactivé par XScreenSaver. Son moteur graphique original
est lancé sous le compte dédié `winlux-saver`, sans modifier l'authentification.
Ce dispositif n'est pas un écran de verrouillage sécurisé.

Les indicateurs réseau, audio et batterie lisent Linux ; l'absence d'une batterie
virtuelle est indiquée comme alimentation secteur. Ils ne reproduisent pas
automatiquement la batterie physique de l'ordinateur hôte.

## Construction et contrôle

Avant `scripts/build-live-iso.sh` :

```sh
python3 scripts/prepare-desktop-packages.py build/desktop-components
```

`python3 -B scripts/test-native-catalogue.py` vérifie le catalogue, les icônes et
l'échappement des chemins sans lancer de VM. Après construction du stage,
`sh scripts/test-native-session-vm.sh` utilise exclusivement un disque jetable,
un affichage invisible en 1920×1080 et une limite de temps. Il contrôle les
fenêtres PE/Linux, l'édition et la sauvegarde, Edge, le catalogue, le menu,
l'animation et la fermeture. Ses captures réelles et journaux sont conservés
sous `build/native-vm.*`. La publication automatise ce contrôle.

La compatibilité générale des EXE, .NET et Photoshop reste indépendante du
gestionnaire de fenêtres. L'affichage natif n'ajoute pas les API Windows manquantes.
