# Session native Winlux

Thunar fournit le service D-Bus `org.xfce.FileManager` utilisé par xfdesktop
pour ouvrir les fichiers et les lanceurs du bureau. Les fichiers de service
seuls (fournis par les données XFCE) ne suffisent pas : la construction exige
également le binaire Thunar et la validation VM active ce service.

Paramètres utilise désormais une fenêtre GTK redimensionnable, une recherche
de catégories et des pages défilantes. Les opérations de contrôle s'exécutent
sans bloquer l'interface ; la page des mises à jour affiche progression et
journal. Les catégories avancées Applications, Programmes Windows,
Accessibilité et Effets visuels ouvrent encore les écrans PE existants.
`WINLUX_LEGACY_SETTINGS=1 winlux-shell-app settings` conserve l'ancien écran.
Le test `xvfb-run -a /usr/bin/python3 -B scripts/test-settings-native.py`
vérifie la navigation sans effectuer d'action système.

Le rendu X11 restaure les zones découvertes depuis son tampon lorsqu'il est
valide, sans demander un redessin GDI complet. Le thème Openbox Winlux partage
les couleurs des Paramètres et agrandit les décorations. Ces changements ne
constituent pas une mesure du débit d'images sur toutes les machines.

La vérification des empreintes du paquet de mise à jour s'effectue en un appel
groupé après validation des chemins. Les vérifications de sécurité et la
sauvegarde de restauration sont conservées.

La session interactive associe Openbox (fenêtres), xfdesktop (bureau),
xfce4-panel (tâches et indicateurs) et un menu GTK inspiré de Windows 11. Les composants sont
préparés depuis les dépôts Ubuntu signés, dans un répertoire isolé : aucun
paquet n'est installé sur l'hôte par le script de préparation.

## Applications

- Explorer et Paramètres sont deux fenêtres PE indépendantes du runtime Winlux.
- Les applications Linux utilisent leurs propres fenêtres et contrôles. Edge
  démarre en fenêtre, avec son bac à sable conservé.
- Les fenêtres EWMH apparaissent dans le panneau, y compris celles du runtime.
- Mousepad est l'[éditeur maintenu par Xfce](https://docs.xfce.org/apps/mousepad/start).
  Les textes peuvent être ouverts, modifiés et sauvegardés ; xterm lance Bash.
- Démarrer propose une recherche, une grille épinglée, « Toutes les applications »
  avec catégories et défilement, les applications récemment lancées et l'alimentation. Le clic
  droit ouvre une application, l'épingle ou crée son raccourci sur le bureau.
  Les épingles sont conservées dans `~/.config/winlux/start.json`. L'apparence
  reprend la disposition Windows 11, avec une implémentation propre à Winlux.
  Les PE sont découverts dans les raccourcis
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

Pour reproduire un diagnostic de programme tiers sans le distribuer dans l'ISO,
définir `WINLUX_EXTERNAL_EXE=/chemin/programme.exe` et éventuellement
`WINLUX_EXTERNAL_DLLS=1` pour les DLL voisines. Le programme est ajouté uniquement
au disque jetable ; son résultat figure dans `chatgpt-result.txt` et le journal
série. Ce diagnostic de chargement ne valide pas une installation MSIX complète.

Le diagnostic de l'installateur utilise 6 Gio parce qu'il embarque une seconde
copie de l'ISO et extrait l'image dans son propre RAM-disque. Ce réglage du test
ne doit pas être confondu avec les essais du bureau natif à 4 Gio.
