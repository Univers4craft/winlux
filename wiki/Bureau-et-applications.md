# Bureau et applications

Le bureau de Winlux reprend l'ergonomie de Windows 11 avec sa propre identité visuelle (« Prisme »). Il repose sur deux piles distinctes, décrites dans [Architecture](Architecture) : la **session native** (composants Linux habillés) et le **shell PE** (vrais `.exe` maison exécutés par le runtime).

## La session native

C'est la session par défaut, en Live comme sur un système installé. Elle est lancée par `scripts/winlux-native-session.sh` et assemble :

| Élément | Composant | Ce qu'il fait |
|---|---|---|
| Bureau | **xfdesktop** | Fond d'écran, icônes, clic droit (avec « Coller » et la création de raccourcis), glisser-déposer. |
| Barre des tâches | **xfce4-panel** | Bouton Démarrer Prisme, un bouton par fenêtre, zone de notification, horloge, indicateurs réseau, volume, batterie. Centrée ou alignée à gauche (`winlux-control align-center` / `align-left`). |
| Fenêtres | **Openbox** | Décorations au thème Winlux, ancrage Windows+flèches, raccourcis. |
| Fichiers | **Thunar** | Fournit le service D-Bus de gestion de fichiers utilisé par le bureau ; l'Explorateur maison reste l'outil principal de l'utilisateur. |
| Menu Démarrer | `scripts/winlux-start-menu.py` (GTK) | Recherche, épingles (`~/.config/winlux/start.json`), « Toutes les applications » avec catégories, recommandations, menu de compte et d'alimentation. Il suit le thème clair ou sombre (contraste vérifié par la CI). |
| Paramètres | `scripts/winlux-settings.py` + `winlux-settings-pages.py` (GTK) | Voir [Paramètres](Parametres). |
| Catalogue d'applications | `scripts/winlux-applications.py` | Crée les lanceurs des programmes Windows trouvés dans les raccourcis `.lnk` et la clé de registre `Uninstall`, avec l'icône extraite du PE ; sert aussi de générateur de vignettes pour les `.exe`. |

### Raccourcis clavier

| Raccourci | Action |
|---|---|
| Windows+Espace (ou clic sur Prisme) | Ouvrir Démarrer |
| Windows+S | Recherche globale |
| Windows+L | Verrouiller |
| Windows+V | Historique du presse-papiers |
| Windows+A / Windows+N | Réglages rapides / notifications |
| Windows+← / → | Ancrer la fenêtre sur une moitié d'écran (avec Ctrl ou Maj : un coin) |
| Ctrl+Maj+Échap | Gestionnaire des tâches |

### Fenêtres de la session native

- **Écran de verrouillage** (`winlux-lock`) : heure et date sur le fond d'écran, puis avatar et mot de passe sur fond flouté. Déclenché par Windows+L, Démarrer › Verrouiller, l'inactivité (5 minutes par défaut) et le réveil de veille. Cinq erreurs bloquent la saisie trente secondes.
- **UAC** (`winlux-uac`) : « Voulez-vous autoriser cette application à apporter des modifications à votre appareil ? », pour les programmes qui demandent les droits administrateur. Voir [Chemins et droits](Chemins-et-droits).
- **Autorisations** (`winlux-permission`) : caméra, micro, capture d'écran, dossiers personnels, demandées une fois par programme.
- **Bienvenue** (`winlux-welcome`) : visite guidée au premier démarrage, lancer un `.exe`, installer les logiciels courants en un clic.
- **Personnalisation** (`winlux-personalize`) : fonds, couleurs, position de la barre des tâches, appliqués à GTK, Openbox et XFCE.
- **Notifications** et **zone de notification** : centre de notifications, icônes `Shell_NotifyIcon` des programmes Windows (`winlux-notify-icon`).
- **Écran d'arrêt**, **sessions multiples** (Verrouiller, Se déconnecter, Changer d'utilisateur ; un Xorg par compte), **signalement de problème** (`winlux-report`).

## Le shell PE : applications maison

Ces applications sont de vrais programmes Windows écrits en Rust `no_std`, compilés en `.exe` (`testexe/`) et exécutés par le runtime. Elles s'ouvrent depuis Démarrer, la session native ou par leur nom de commande Windows (`taskmgr`, `regedit`, `taskschd.msc`, `wf.msc`…).

| Application | Fichier | Points forts |
|---|---|---|
| **Explorateur** | `shell/explorer.rs` | Onglets, double volet, vues, tri, recherche, copier/couper/coller, corbeille, « Ce PC » avec lettres de lecteur, partage de dossier, ouverture des archives ZIP/7z/RAR/tar et « Extraire tout », images disque montées comme lecteur. |
| **Gestionnaire des tâches** | `shell/taskmgr.rs` | Processus groupés (avec les `.exe` reconnus), fin de tâche et priorité réelles, graphiques processeur/mémoire/disque/réseau/GPU, historique, démarrage, utilisateurs, services. |
| **Éditeur du Registre** | `shell/regedit.rs` | Arborescence, édition des valeurs, import/export `.reg`. |
| **Observateur d'événements** | `shell/eventviewer.rs` | Journaux système, applications et `.exe`, filtres, recherche, export, bouton « Signaler ». |
| **Informations système** | `shell/sysinfo.rs` | Carte mère, processeur, mémoire, disques, GPU, réseau, périphériques, recherche. |
| **Planificateur de tâches** | `shell/taskschd.rs` | Déclencheurs horaires, au démarrage, à l'ouverture de session ou sur événement (réseau, alimentation, USB). |
| **Pare-feu** | `shell/firewall.rs` | Profils, règles par programme, ports ouverts avec le nom de l'appli. |
| **Gestion des disques** | `shell/diskmgr.rs` | Volumes, lettres, bouton « Réparer » pour un NTFS laissé ouvert par Windows. |
| **Nettoyage de disque**, **Historique des fichiers**, **Restauration** | `diskcleaner.rs`, `filehistory.rs`, `restore.rs` | Ménage, sauvegardes incrémentales, points de restauration. |
| **Store** | `shell/store.rs` | Bibliothèque unique APT, Flatpak, Snap, AppImage et Windows (catalogue Scoop Extras par défaut). |
| **Polices** | `shell/fonts.rs` | Gestionnaire de polices. |
| **Applications installées** | `shell/programs.rs` | Programmes Windows installés (MSI, NSIS, Inno) et désinstallation. |
| **Terminal** | `shell/terminal.rs` (+ xterm) | Terminal Winlux avec Bash ; ouvert automatiquement pour les `.exe` console. |
| **Lecteur multimédia** | `shell/mediaplayer.rs` | Audio et vidéo via FFmpeg (H.264, HEVC, AV1…), sous-titres, aperçu au survol. |
| **Visionneuse d'images**, **Lecteur PDF** | `imageviewer.rs`, `pdfviewer.rs` | Décodeurs d'images maison, miniatures dans l'Explorateur. |
| **Petits outils** | `calc.rs`, `screenshot.rs`, `snip.rs`, `cliphist.rs`, `voicerecorder.rs`, `postit.rs`, `notepad.rs`, `weather.rs` | Calculatrice, capture et enregistrement d'écran, historique du presse-papiers, enregistreur vocal, post-it, éditeur de texte, météo. |

L'ancien bureau entièrement PE (`gui_demo.exe` avec `WINLUX_DESKTOP_DEMO=1`, code dans `shell/desktop.rs` et `shell/wm.rs`) n'est plus la session par défaut, mais reste dans les tests et sert de banc d'essai au runtime.

## Applications Linux

Les applications Linux s'installent normalement (`apt`, Store, Flatpak, Snap, AppImage). Leurs fichiers `.desktop` sont reconnus par Démarrer, et un crochet APT actualise le catalogue après chaque installation. Un navigateur web est préparé hors ligne dans l'ISO (Paramètres › Navigateurs permet d'en choisir un autre et de le mettre par défaut) ; il tourne sous un compte isolé, et ses téléchargements arrivent dans `C:\Users\<nom>\Downloads` (voir [docs/BROWSERS.md](https://github.com/Univers4craft/winlux/blob/master/docs/BROWSERS.md)).

## Échelle d'affichage

Le runtime choisit l'échelle comme Windows (paliers de 100 à 300 %) : réglage `Xft.dpi` s'il existe, sinon taille physique de l'écran lue dans l'EDID par RandR, sinon définition. Les applications maison et les programmes Windows sont ainsi lisibles du petit portable à l'écran 4K.

## Langues

Le bureau, l'Explorateur, les Paramètres, l'écran de verrouillage et les applications sont traduits en **français, anglais, espagnol et allemand**. Le changement de langue est immédiat (Paramètres › Localisation). Quelques applications restent à traduire (issue #139).

## Ce qui reste à faire

- Corbeille complète (restaurer, vider) et barre de progression des copies (#10, #11).
- Navigation au clavier (Tab) dans toutes les fenêtres du shell.
- Dans la session native : aperçu d'ancrage à la souris (Snap Layouts), animations, bureaux virtuels et widgets plus proches de Windows 11 (ils existent dans l'ancien bureau PE).
- Écran de chargement du noyau, sons système et jeu d'icônes à l'identité Prisme (#170).

Voir aussi : [Paramètres](Parametres) · [INTERFACE.md](https://github.com/Univers4craft/winlux/blob/master/INTERFACE.md) · [docs/NATIVE-DESKTOP.md](https://github.com/Univers4craft/winlux/blob/master/docs/NATIVE-DESKTOP.md)
