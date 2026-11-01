# Session native Winlux

Ancrage : Windows+Flèche gauche/droite place la fenêtre sur une moitié de
l'écran ; Windows+Espace ouvre Démarrer. La capture de Windows seule a été
retirée car elle bloquait les combinaisons ; les profils Winlux existants sont
migrés au démarrage sans modifier les raccourcis personnalisés.

`xvfb-run -a -s '-screen 0 1920x1080x24 -noreset' /usr/bin/python3 -B scripts/test-window-performance.py`
vérifie les deux ancrages et mesure douze déplacements/redimensionnements
(médiane, percentile 95, maximum). Ces délais incluent les processus de test et
les allers-retours X11, et ne mesurent pas le rythme des images affichées.
La validation VM produit aussi `window-latency.json` avec la fenêtre Terminal.

Thunar fournit le service D-Bus `org.xfce.FileManager` utilisé par xfdesktop
pour ouvrir les fichiers et les lanceurs du bureau. Les fichiers de service
seuls (fournis par les données XFCE) ne suffisent pas : la construction exige
également le binaire Thunar et la validation VM active ce service.

Paramètres utilise désormais une fenêtre GTK redimensionnable, une recherche
de catégories et des pages défilantes. Les opérations de contrôle s'exécutent
sans bloquer l'interface ; la page des mises à jour affiche progression et
journal. Les pages Programmes Windows, Accessibilité, Effets visuels,
Localisation, Gestionnaire de fichiers, Son avancé, Mises à jour système,
Lecteurs réseau, Personnalisation du bureau et Bluetooth et appareils viennent
de `winlux-settings-pages` (installé à côté de `winlux-settings`) et agissent
sur la session réelle : BlueZ, pactl, winlux-sysupdate, winlux-drives,
registre Windows (`Explorer\Advanced`, relu par l'Explorateur), xsettings et
Openbox, `accessibility.ini` et `<programme>.exe.winlux.ini` partagés avec le
runtime. Une page sans service ou sans matériel n'apparaît pas dans la liste.
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
XScreenSaver ne verrouille rien : c'est le rôle de `winlux-lock`.

## Écran de verrouillage

`winlux-lock` (scripts/winlux-lock.py) affiche un écran de verrouillage au
style de Windows 11 : heure et date sur le fond d'écran, puis, au premier
appui ou clic, l'avatar, le nom du compte et le champ mot de passe sur un fond
flouté. Il couvre tous les écrans et prend clavier et souris, donc les
raccourcis du bureau sont inactifs tant qu'il est affiché. Après cinq erreurs,
la saisie est bloquée trente secondes.

- Déclenchement : Windows+L (Openbox), Démarrer > alimentation > Verrouiller,
  Paramètres > Confidentialité et sécurité, inactivité (`winlux-lock daemon`,
  extension X MIT-SCREEN-SAVER) et mise en veille. Le module d'alimentation
  du runtime affiche l'écran avant d'écrire dans `/sys/power/state`, pour que
  le bureau ne soit jamais visible au réveil ; le service détecte aussi une
  veille déclenchée ailleurs (écart CLOCK_BOOTTIME / CLOCK_MONOTONIC).
- Mot de passe : celui du compte local. La session tournant encore en
  administrateur, le compte protégé est le compte personnel créé à
  l'installation (premier UID ≥ 1000), vérifié avec `crypt()` contre
  `/etc/shadow` ; en session Live, c'est le mot de passe de
  `/etc/winlux/users.conf`. Un compte sans mot de passe se déverrouille d'un
  clic et ne se verrouille jamais tout seul.
- Réglages : `/etc/winlux/lock.conf` (`LOCK_AFTER` en secondes, 0 = jamais,
  5 minutes par défaut ; `LOCK_ON_RESUME=1`), via `winlux-control
  lock-after-<s>`, `lock-resume-on|off` et `lock-now`.
- Limites : un administrateur qui tue le processus lève le verrouillage, et
  Ctrl+Alt+F1… change toujours de terminal virtuel. Le bureau PE historique
  (gui_demo.exe) garde son propre écran de connexion (`login.rs`).
- Test hors affichage : `python3 -B scripts/test-lock.py`.

## Autorisations des applications Windows

Comme sous Windows 11, une application `.exe` demande la première fois
qu'elle veut accéder à quelque chose de sensible. La fenêtre
`winlux-permission ask` (Autoriser / Ne pas autoriser, refus par défaut au
clavier, fermée sans réponse au bout de deux minutes) explique en une phrase
ce que l'application pourra faire. Le choix est retenu par programme dans
`~/.config/winlux/permissions.tsv` et se change dans Paramètres >
Confidentialité et sécurité (interrupteur par autorisation, « Tout
redemander »). Aucun fichier n'est analysé : ce n'est pas un antivirus.

| Autorisation | Contrôle dans le runtime (`runtime/src/permissions.rs`) | Demandée ? |
|---|---|---|
| Fichiers personnels (Bureau, Documents, Images, Musique, Vidéos, Téléchargements, d'après `user-dirs.dirs`) | conversion des chemins Windows (`paths::to_linux`) ; un refus rend « fichier introuvable » | oui |
| Caméra | énumération V4L2 (`v4l2::camera_allowed`) | oui |
| Micro | waveIn, DirectSound et WASAPI (`audio::subscribe_microphone`) ; un refus donne du silence | oui |
| Capture de l'écran | `gui::capture_screen` | oui |
| Réseau et Internet | `connect`/`sendto` hors boucle locale, WinHTTP | non : autorisé d'office, mais listé et blocable |

- Restent libres sans question : le dossier de l'application (un installateur
  lancé depuis Téléchargements lit ce dossier) et le dossier des fichiers
  ouverts par double-clic ou glisser-déposer (arguments de la ligne de
  commande).
- Les interrupteurs « Caméra / Micro pour toutes les applications »
  (`/etc/winlux/privacy.conf`) l'emportent sur les choix par application.
- Les connexions entrantes restent gérées par l'alerte du pare-feu.
- Les programmes livrés avec Winlux (`/opt/winlux`, `/usr/lib/winlux`) ne
  demandent rien. Sans affichage ou sans `winlux-permission`, le comportement
  précédent s'applique (accès autorisé, rien n'est retenu).
- Tests : `cargo test -p winlux permissions`, `python3 -B scripts/test-permission.py`.

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
