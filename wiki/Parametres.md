# Paramètres

**Paramètres** est l'application de réglages de Winlux, sur le modèle de celle de Windows 11 : une liste de catégories à gauche avec recherche, des pages défilantes à droite. Dans la session native, c'est une fenêtre GTK (`scripts/winlux-settings.py`, complétée par `scripts/winlux-settings-pages.py`) ; les actions système passent par `winlux-control` et s'exécutent sans bloquer l'interface.

## Deux règles

1. **Masqué plutôt que grisé.** Une page ou une section qui ne correspond à rien sur la machine n'apparaît pas : pas de page Bluetooth sans adaptateur ni périphérique, pas de page Son sans carte son, pas de Lecteurs réseau sans outil de montage, pas de réglage CPU que le matériel ne permet pas. La détection est faite au démarrage par `winlux-hardware`.
2. **Les réglages matériels avancés vivent ici, et seulement ici** (processeur, GPU, alimentation), après détection réelle du matériel.

## Les 29 pages

| # | Page | Ce qu'on y règle |
|---|---|---|
| 0 | **Système** | Vue d'ensemble de la machine et accès aux outils système. |
| 1 | **Applications** | Applications installées (APT, Flatpak…), applications par défaut, démarrage automatique. |
| 2 | **Réseau** | Ethernet, Wi-Fi, état de la connexion. |
| 3 | **Son** | Sortie, entrée, volume. |
| 4 | **Clavier** | Disposition (AZERTY, QWERTY, QWERTZ…), répétition. |
| 5 | **Souris** | Vitesse, boutons, pavé tactile (appui pour cliquer). |
| 6 | **Personnalisation** | Thème clair/sombre, couleur d'accent. |
| 7 | **Mises à jour** | Rechercher et installer les mises à jour de Winlux, mises à jour automatiques, retour arrière. |
| 8 | **Écran** | Définition, échelle, « Suivre la taille de la VM » (seulement en machine virtuelle). |
| 9 | **À propos** | Version, commit et date de construction (`BUILD-INFO`), matériel réel, en quatre langues. |
| 10 | **Programmes Windows** | Réglages par `.exe` (fichiers `<programme>.exe.winlux.ini` partagés avec le runtime). |
| 11 | **Navigateurs** | Installer un navigateur préparé hors ligne, choisir le navigateur par défaut. |
| 12 | **Accessibilité** | Loupe, Narrateur, contraste élevé, aides clavier. |
| 13 | **Effets visuels** | Animations, transparence, réglages de l'Explorateur écrits dans le registre (`Explorer\Advanced`). |
| 14 | **Localisation** | Langue (FR/EN/ES/DE, changement immédiat), formats, fuseau horaire. |
| 15 | **Gestionnaire de fichiers** | Fichiers cachés, extensions, comportement de l'Explorateur. |
| 16 | **Son avancé** | Volume par application, périphériques (PipeWire). |
| 17 | **Mises à jour système** | Mises à jour des paquets Ubuntu (APT), planifiables. |
| 18 | **Confidentialité et sécurité** | Verrouillage (délai, au réveil), autorisations des applications (caméra, micro, écran, fichiers, réseau), « Tout redemander ». |
| 19 | **Processeur** | Profils de performance, gouverneur, mode jeu, temps de démarrage mesuré, affichés selon le matériel. |
| 20 | **Lecteurs réseau** | SMB, NFS, SFTP, FTP, WebDAV, S3 montés comme un disque. |
| 21 | **Personnalisation du bureau** | Fond d'écran, couleurs, position de la barre des tâches (centrée ou à gauche, haut, bas…). |
| 22 | **Bluetooth et appareils** | Appairage BlueZ, manettes, caméras, micros, périphériques d'entrée. |
| 23 | **Alimentation et batterie** | Mise en veille, écran, plans d'alimentation, batterie (affichée seulement si présente). |
| 24 | **Graphismes** | GPU (y compris hybrides), Direct3D 9/12 et backend Vulkan, décodage vidéo matériel, overlay de performances. |
| 25 | **Signaler un problème** | Nouveau signalement, dernier plantage, rapports enregistrés, compte GitHub facultatif. |
| 26 | **Imprimantes et scanners** | Ajout d'imprimantes CUPS, page de test, imprimer en PDF. |
| 27 | **Bureau à distance** | Serveur RFB/VNC de la session. |
| 28 | **Partage et transfert** | Dossiers partagés (Samba) et transfert des réglages par fichier `.wlxprofile`. |

Les pages 25 à 28 ouvrent l'interface complète du shell PE (`testexe/src/shell/settings.rs` et ses modules), qui reste aussi l'écran des Paramètres de l'ancien bureau PE (`WINLUX_LEGACY_SETTINGS=1 winlux-shell-app settings`).

## Sous le capot

- **`winlux-control`** (`scripts/winlux-control.sh`) applique les actions : `align-left`/`align-center`, `lang-*`, `keyboard-*`, `tz-*`, `lock-after-<s>`, `update-apply`, `share-*`, etc. Il publie l'état de la machine que les pages lisent.
- Les réglages persistants sont dans `/etc/winlux/*.conf` (affichage, entrées, verrouillage, confidentialité, volumes…), dans le profil (`~/.config/winlux/`), dans Xfconf pour la session XFCE et dans le [registre](Registre-Windows) pour ce que lisent les programmes Windows.
- Ajouter une page : prendre le numéro suivant, augmenter `PAGE_COUNT`, l'insérer dans l'ordre de navigation, ajouter son libellé traduit dans les **quatre langues** (`shell/locale.rs` pour le shell PE, table de traduction pour la session GTK), puis vérifier la navigation avec `scripts/test-settings-native.py`. Les numéros de pages bougent quand plusieurs pull requests en ajoutent : vérifiez toujours `master` avant.

## Tests

```sh
xvfb-run -a /usr/bin/python3 -B scripts/test-settings-native.py   # navigation, sans action système
python3 -B scripts/capture-ui.py                                  # captures de toutes les pages
```

La CI photographie toutes les pages à chaque pull request et signale celles qui ont changé de plus de 2 % des pixels (voir [Tests et CI](Tests-et-CI)).

Voir aussi : [Bureau et applications](Bureau-et-applications) · [docs/DESKTOP-FEATURES.md](https://github.com/Univers4craft/winlux/blob/master/docs/DESKTOP-FEATURES.md)
