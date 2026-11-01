<p align="center">
  <img src="integration/native/start.svg" alt="Logo Prisme de Winlux" width="96">
</p>

<h1 align="center">Winlux</h1>

<p align="center">
  <strong>Un système d'exploitation sur noyau Linux où les programmes Windows (.exe) se lancent d'un double-clic, sans Wine et sans machine virtuelle cachée.</strong>
</p>

<p align="center">
  <a href="https://github.com/Univers4craft/winlux/actions/workflows/ci.yml"><img src="https://github.com/Univers4craft/winlux/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/Univers4craft/winlux/releases/latest"><img src="https://img.shields.io/github/v/release/Univers4craft/winlux?label=derni%C3%A8re%20version" alt="Dernière version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-GPL--3.0--or--later-blue" alt="Licence GPL 3 ou ultérieure"></a>
</p>

<p align="center">
  <a href="wiki/Home.md"><b>Wiki complet</b></a> ·
  <a href="https://github.com/Univers4craft/winlux/releases/latest">Télécharger l'ISO</a> ·
  <a href="Suivi.md">Suivi des tâches</a> ·
  <a href="COMPATIBILITY.md">Compatibilité</a> ·
  <a href="docs/BUILD.md">Compiler</a> ·
  <a href="https://github.com/Univers4craft/winlux/issues">Issues</a>
</p>

---

> [!IMPORTANT]
> **Winlux est une préversion technique.** Il démarre, s'installe et se met à jour, et beaucoup de choses fonctionnent déjà, mais **tous les `.exe` ne se lancent pas** et les gros logiciels Windows (Paint.NET, Photoshop, jeux DirectX récents, applications WPF) ne sont pas encore utilisables. Testez-le d'abord dans une machine virtuelle, ou sur un PC qui ne contient pas de données importantes.

## Sommaire

1. [Qu'est-ce que Winlux ?](#quest-ce-que-winlux-)
2. [Les principes du projet](#les-principes-du-projet)
3. [État actuel en un coup d'œil](#état-actuel-en-un-coup-dœil)
4. [Comment ça marche](#comment-ça-marche)
5. [Ce qui fonctionne aujourd'hui](#ce-qui-fonctionne-aujourdhui)
6. [Ce qui ne fonctionne pas encore](#ce-qui-ne-fonctionne-pas-encore)
7. [Installer Winlux](#installer-winlux)
8. [Mettre à jour](#mettre-à-jour)
9. [Utiliser seulement le runtime sur un Linux existant](#utiliser-seulement-le-runtime-sur-un-linux-existant)
10. [Compiler et tester](#compiler-et-tester)
11. [Organisation du dépôt](#organisation-du-dépôt)
12. [Documentation](#documentation)
13. [Contribuer et signaler un problème](#contribuer-et-signaler-un-problème)
14. [Licence](#licence)

---

## Qu'est-ce que Winlux ?

Winlux est un système d'exploitation complet, distribué sous forme d'**ISO installable**, qui réunit trois choses :

1. **Un vrai noyau Linux** et une base Ubuntu 26.04 LTS avec `apt` et `dpkg` natifs. Les logiciels Linux s'installent et fonctionnent normalement.
2. **Une couche de compatibilité Windows écrite de zéro en Rust** (le *runtime* Winlux) : elle charge directement les exécutables Windows 64 bits (format PE32+) et leur fournit les DLL Windows dont ils ont besoin (`kernel32`, `user32`, `gdi32`, `ntdll`, `advapi32`, `ole32`, `ws2_32`, Direct3D, etc.), réimplémentées au-dessus de Linux.
3. **Un bureau qui ressemble à Windows 11** : menu Démarrer centré (ou à gauche), barre des tâches, Explorateur, Paramètres, écran de verrouillage, gestionnaire des tâches, sans aucun logo ni marque Microsoft.

**Le but** : qu'un utilisateur habitué à Windows puisse double-cliquer sur un `.exe` n'importe où (bureau, clé USB, dossier Téléchargements) et qu'il s'ouvre, comme sous Windows, tout en profitant d'un système Linux avec un accès complet à ses fichiers. Il n'y a **qu'un seul noyau** : aucune machine virtuelle Windows ne tourne en arrière-plan, et aucune licence Windows n'est nécessaire.

## Les principes du projet

Ces règles guident chaque décision. Elles sont détaillées dans la page [But et principes](wiki/But-et-principes.md) du wiki.

| Principe | Ce que ça veut dire concrètement |
|---|---|
| **Tout est fait maison** | La couche de compatibilité Windows n'utilise **ni Wine, ni Proton, ni DXVK**, ni aucune couche tierce. Une étape de la CI refuse toute dépendance à Wine. Les briques Linux standard restent permises : FFmpeg pour les médias, CUPS pour l'impression, Samba pour le partage, `mount.cifs`/`sshfs`/`rclone` pour les lecteurs réseau. |
| **Aucun lien avec Microsoft** | En dehors de la compatibilité `.exe`, Winlux n'est pas associé à Microsoft : **comptes locaux uniquement**, aucun compte en ligne, aucune application Microsoft proposée par défaut. Seule exception assumée : le démarrage Secure Boot passe par le *shim* contresigné par l'autorité UEFI tierce, la seule façon de démarrer sur un PC du commerce sans réglage. |
| **Identité visuelle propre** | L'ergonomie est celle de Windows 11, mais il n'y a **aucun logo Windows**. Le logo de Winlux s'appelle **« Prisme »** : un losange taillé, violet à gauche (Linux) et cyan à droite (*lux*, la lumière). |
| **Paramètres honnêtes** | Une section qui ne correspond à rien sur la machine (pas de Bluetooth, pas de batterie, pas de réglage CPU possible) est **masquée**, jamais grisée ni vide. Les réglages avancés du matériel vivent uniquement dans Paramètres. |
| **Ne rien promettre qui n'existe pas** | Une fonction partielle est documentée comme partielle. Un import Windows non implémenté arrête le programme avec un message clair plutôt que de faire semblant de réussir. |
| **Multilingue** | Le bureau, l'Explorateur, les Paramètres et les applications intégrées sont traduits en **français, anglais, espagnol et allemand** ; l'installateur propose aussi l'italien et le portugais. |

## État actuel en un coup d'œil

| | |
|---|---|
| **Dernière version publiée** | [**v0.2.12**](https://github.com/Univers4craft/winlux/releases/tag/v0.2.12) (1er novembre 2026) : ISO de 1,92 Gio, première version sur la base Ubuntu 26.04 avec APT natif. |
| **Version en préparation** | **v0.2.13** : changement rapide d'utilisateur, impression, son PipeWire, lecteurs réseau SMB/NFS, et surtout un outil de mise à jour corrigé. |
| **Architecture prise en charge** | PC x86-64, démarrage BIOS ou UEFI, Secure Boot compatible. |
| **Programmes Windows** | Exécutables **64 bits** uniquement. Les `.exe` 32 bits sont détectés et refusés avec un message explicite. Les programmes .NET 32 et 64 bits passent par le CLR maison. |
| **Validation** | Chaque pull request passe une CI de plusieurs jobs (formatage, Clippy, tests unitaires, tests de PE réels, tests C MinGW et MSVC, captures d'écran comparées). Chaque version est testée en VM (Live, installation BIOS/UEFI, mise à jour) avant publication. |
| **Matériel réel** | Testé surtout en machine virtuelle (QEMU/KVM, VirtualBox, VMware). La validation sur une large gamme de PC réels reste à faire. |

> [!WARNING]
> **Mise à jour depuis une ancienne version.** Une installation **v0.2.6 ou plus ancienne** ne peut pas passer à la nouvelle base en place : il faut la **réinstaller une fois** avec l'ISO récente. Une installation **v0.2.12** devra aussi être réinstallée une fois avec l'ISO v0.2.13, car l'outil de mise à jour de la v0.2.12 contient un défaut qui l'empêche d'installer quoi que ce soit (corrigé dans la v0.2.13). Ensuite, les mises à jour se font depuis **Paramètres › Mises à jour**, sans réinstaller. Vos documents ne sont pas concernés par ce défaut, mais sauvegardez-les avant toute réinstallation.

## Comment ça marche

```
┌────────────────────────────────────────────────────────────────────────┐
│ Bureau façon Windows 11                                                │
│  Session native : xfdesktop + xfce4-panel + Openbox + Thunar, habillés │
│  Menu Démarrer GTK, Paramètres GTK, écran de verrouillage, UAC…        │
│  Applications maison PE : Explorateur, Gestionnaire des tâches,        │
│  Éditeur du Registre, Store, Observateur d'événements…                 │
├────────────────────────────────────────────────────────────────────────┤
│ Runtime Winlux (Rust, maison)            │ Applications Linux          │
│  Chargeur PE32+, PEB/TEB, TLS, SEH       │  apt / dpkg, .deb, Flatpak, │
│  ~100 DLL Windows réimplémentées         │  Snap, AppImage             │
│  Registre, chemins C:\ ↔ /home, UAC      │                             │
│  CLR .NET maison, msiexec, NSIS/Inno     │                             │
│  GDI/X11, Direct2D, D3D9/11/12 → GL/VK   │                             │
├────────────────────────────────────────────────────────────────────────┤
│ Base Ubuntu 26.04 LTS : Xorg, PipeWire/ALSA, NetworkManager, CUPS,     │
│ Samba, FFmpeg, BlueZ… (paquets Ubuntu signés, gérés par dpkg)          │
├────────────────────────────────────────────────────────────────────────┤
│ Noyau Linux (un seul noyau, aucune VM Windows)                         │
└────────────────────────────────────────────────────────────────────────┘
```

Quand vous double-cliquez sur `programme.exe` :

1. Le fichier est associé au type MIME `application/x-ms-dos-executable` (et enregistré auprès de `binfmt_misc`), ce qui appelle le lanceur `winlux-open`.
2. `winlux-open` regarde si le programme est en mode console (il ouvre alors le Terminal Winlux) ou graphique, puis lance `/usr/bin/winlux programme.exe`.
3. Le runtime lit les en-têtes PE, mappe les sections en mémoire, applique les relocations et résout les imports. Chaque DLL Windows demandée (`kernel32.dll`, `user32.dll`…) est fournie par une implémentation Rust intégrée ; les DLL livrées avec le programme sont chargées pour de vrai.
4. Il prépare les structures attendues par un programme Windows (PEB, TEB via le registre `GS`, TLS, environnement, ligne de commande), lit le manifeste (élévation UAC, DPI) puis saute au point d'entrée.
5. Chaque appel à l'API Windows est traduit en appel Linux : fichiers POSIX avec traduction des chemins `C:\…`, fenêtres X11, son PipeWire/ALSA, sockets, OpenGL/Vulkan pour Direct3D, CUPS pour l'impression.

Le détail de chaque couche est expliqué dans la page [Architecture](wiki/Architecture.md) du wiki.

## Ce qui fonctionne aujourd'hui

Cette liste décrit le code présent sur `master`. Certaines fonctions sont plus récentes que la dernière ISO publiée.

### Système et installation

- **ISO Live hybride** (BIOS, UEFI, clé USB) avec racine SquashFS compressée en xz et couche d'écriture en mémoire.
- **Installateur graphique** : installation automatique sur tout le disque par défaut, partitionnement manuel complet (créer, supprimer, redimensionner, formater) derrière un mode avancé, **chiffrement LUKS**, double démarrage avec un Windows existant, compte local avec avatar, Wi-Fi et proxy, import des données d'une partition Windows, test des performances du matériel et profils d'utilisation, installation silencieuse par fichier de réponses. L'installateur s'adapte à la définition et au DPI de l'écran, du 1366×768 à la 4K. Six langues.
- **Secure Boot** : chaîne shim → GRUB → noyau signés, avec la possibilité d'utiliser ses propres clés ([docs/SECURE-BOOT.md](docs/SECURE-BOOT.md)).
- **Mode récupération** dans le menu GRUB : réparer le démarrage, revenir au point de restauration précédent, terminal de secours, vérification du système de fichiers ([docs/RECOVERY.md](docs/RECOVERY.md)).
- **Mises à jour** depuis les releases GitHub publiques, vérifiées par SHA-256, avec point de restauration et retour arrière.
- **Détection du matériel** au démarrage (GPU, hyperviseur, Bluetooth, batterie, capteurs) ; pilotes d'affichage pour QEMU, VirtualBox et VMware.
- Plusieurs utilisateurs connectés en même temps avec changement rapide, **bureau à distance** (serveur RFB/VNC maison), **historique des fichiers**, points de restauration, écran bleu de diagnostic et journal centralisé.

### Bureau

- Menu Démarrer façon Windows 11 : recherche, épingles, « Toutes les applications » avec catégories, recommandations, menu d'alimentation et de compte. Les programmes Windows installés y apparaissent automatiquement, avec leur icône extraite du `.exe`.
- Échelle d'affichage automatique selon la taille physique réelle de l'écran (paliers Windows de 100 à 300 %), du portable 1366×768 à l'écran 4K.
- Barre des tâches centrée ou alignée à gauche, zone de notification avec les icônes des applications Windows (`Shell_NotifyIcon`), réglages rapides réseau et volume.
- **Explorateur** (application maison) : onglets, double volet, recherche, tri, copier/couper/coller, corbeille, ouverture des **archives ZIP, 7z, RAR et tar**, « Extraire tout », partage de dossier en un clic droit.
- Bureau avec glisser-déposer et « Coller » ; un `.exe` posé sur le bureau garde son icône.
- **Paramètres** : 29 pages, dont Système, Réseau, Son, Écran, Personnalisation, Bluetooth, Alimentation et batterie, Graphismes, Processeur, Confidentialité et sécurité, Lecteurs réseau, Imprimantes, Partage et transfert, Mises à jour… ([détail](wiki/Parametres.md)).
- Écran de verrouillage (Windows+L, inactivité, réveil), fenêtre **UAC** pour les programmes qui demandent les droits administrateur, **autorisations par application** (caméra, micro, capture d'écran, dossiers personnels).
- Applications intégrées : Gestionnaire des tâches (Ctrl+Maj+Échap), Éditeur du Registre, Observateur d'événements, Informations système, Planificateur de tâches, Pare-feu, Gestion des disques, Nettoyage de disque, Store, Gestionnaire de polices, Terminal, lecteur multimédia (FFmpeg), visionneuse d'images, lecteur PDF, calculatrice, capture d'écran et enregistrement, historique du presse-papiers (Windows+V), enregistreur vocal, post-it.
- Accessibilité : loupe, Narrateur, contraste élevé, aides clavier.
- Identité « Prisme » : logo, menu GRUB, écran de démarrage, fonds « Winlux Nuit », « Aube » et « classique », son de démarrage.

### Compatibilité Windows

- Chargeur **PE32+ x86-64** : sections, relocations, imports et imports différés, vraies DLL chargées avec `LoadLibrary`, callbacks TLS, `DllMain`, ressources, manifestes.
- Environ **100 modules de DLL** réimplémentés, dont `kernel32`, `ntdll`, `user32`, `gdi32`, `advapi32`, `shell32`, `shlwapi`, `ole32`, `oleaut32`, `comctl32`, `comdlg32`, `ws2_32`, `winhttp`, `iphlpapi`, `bcrypt`, `crypt32`, `winmm`, `dsound`, `xaudio2`, `mmdevapi`, `xinput`, `dinput8`, `winspool`, `msvcrt`/`ucrtbase`, `libstdc++`/`libgcc` (MinGW).
- **Exceptions** : SEH x64 et exceptions C++ MSVC (`__CxxFrameHandler3`, `__try/__except/__finally`).
- **Graphismes** : fenêtres et messages Win32 sur X11, GDI avec polices Xft, **Direct2D** et **DirectWrite** logiciels maison, **WIC** (BMP, PNG, JPEG, GIF, TIFF, WebP…), **Direct3D 11 traduit vers OpenGL**, **Direct3D 9 et 12** avec un backend **Vulkan**, décodage vidéo matériel DXVA2 (H.264).
- **Son** : waveOut/waveIn, DirectSound, WASAPI et XAudio2 vers PipeWire ou ALSA. **Périphériques** : manettes XInput/DirectInput, webcam (Media Foundation), micro.
- **Registre Windows** persistant partagé entre toutes les applications, avec `reg.exe`, `regedit.exe` et l'import/export de fichiers `.reg`.
- **Installateurs** : `msiexec` intégré pour les `.msi`, extraction des installateurs **NSIS** et **Inno Setup**, page « Applications installées ».
- **.NET** : un CLR maison exécute les programmes .NET simples (interpréteur CIL, ramasse-miettes, exceptions, P/Invoke). Une application .NET autonome qui embarque son propre `coreclr.dll` est démarrée par ce runtime embarqué.
- **Intégration Linux** : `C:\` = dossier personnel, `Z:\` = racine Linux, lettres `E:`, `F:`… pour les clés USB, disques et lecteurs réseau ; presse-papiers et glisser-déposer communs ; lancement croisé Windows ↔ Linux ; impression vers CUPS ; ports réseau ouverts par les `.exe` visibles dans le pare-feu.

Ce qu'il reste à faire, mesuré et classé par impact, est dans [docs/COMPATIBILITE.md](docs/COMPATIBILITE.md). La liste détaillée, API par API, est dans [COMPATIBILITY.md](COMPATIBILITY.md) et dans la page [Compatibilité des applications](wiki/Compatibilite-des-applications.md).

## Ce qui ne fonctionne pas encore

Liste honnête des limites actuelles, tenue à jour dans [Suivi.md](Suivi.md) :

- **Programmes 32 bits** : non pris en charge (sauf .NET managé).
- **Gros logiciels** : Paint.NET démarre son runtime .NET mais ne s'ouvre pas encore ([docs/PAINTNET.md](docs/PAINTNET.md)) ; WPF et WinForms ne sont pas encore disponibles ; les clients Windows de ChatGPT et Claude ne démarrent pas ([docs/CLAUDE-CHATGPT-WINDOWS.md](docs/CLAUDE-CHATGPT-WINDOWS.md)) ; Photoshop et les jeux Direct3D récents ne sont pas utilisables.
- **CLR maison** : pas encore de génériques de la bibliothèque de base (`List<T>`, `Dictionary`), ni de délégués ou LINQ.
- **COM** : pas d'activation depuis le registre, ni de serveurs hors processus ni d'automatisation. **Media Foundation et DirectShow** ne décodent pas encore la vidéo pour les `.exe` (le lecteur multimédia intégré, lui, lit tous les formats).
- **Processus** : pas d'héritage des handles entre processus ; `FreeLibrary` ne décharge pas réellement les DLL ; services Windows sans `StartServiceCtrlDispatcher`.
- **Sécurité** : le bureau de l'ISO doit encore passer en utilisateur non privilégié ; les mises à jour sont vérifiées par SHA-256 mais **pas encore signées** (Ed25519 prévu) ; pas encore de partitions système A/B résistantes à une coupure de courant ([docs/AB_UPDATES.md](docs/AB_UPDATES.md)).
- **Bureau** : corbeille encore incomplète (restaurer, vider), pas de barre de progression pour les copies, navigation clavier à compléter, identité visuelle à terminer (écran de chargement du noyau, sons système, jeu d'icônes).
- **Matériel réel** : veille prolongée, capot de portable et large gamme de PC encore à valider.

## Installer Winlux

### Dans une machine virtuelle (recommandé pour découvrir)

1. Téléchargez `winlux-live-<version>.iso` depuis la [dernière release](https://github.com/Univers4craft/winlux/releases/latest) et vérifiez sa somme SHA-256 avec le fichier `.sha256` publié à côté.
2. Créez une VM **x86-64** avec au moins **4 Gio de RAM**, **2 processeurs** et **24 Gio de disque**, en BIOS ou en UEFI. QEMU/KVM, VirtualBox et VMware sont pris en charge (affichage par `bochs`, `qxl` ou `vmwgfx`).
3. Démarrez sur l'ISO : la session Live s'ouvre directement sur le bureau. Rien n'est écrit sur le disque tant que vous n'installez pas.
4. Lancez **« Installer Winlux »** depuis le bureau, choisissez la langue et le clavier, puis laissez l'installation automatique utiliser tout le disque virtuel.
5. Au premier démarrage, l'assistant vous fait créer votre **compte local**, choisir le thème, le fond d'écran et les applications à installer.

### Sur un vrai PC

L'installateur sait installer Winlux à côté de Windows ou sur un disque entier, mais **sauvegardez vos données avant**. Préparez une clé USB avec l'ISO (Balena Etcher, Ventoy, ou `dd if=winlux-live-<version>.iso of=/dev/sdX bs=4M status=progress`), démarrez dessus et suivez le même parcours. Lisez [les protections de l'installateur](docs/INSTALLER-SAFETY.md) avant de toucher à un disque contenant des données.

Le guide pas à pas, avec la configuration matérielle conseillée, le double démarrage et le dépannage, est dans la page [Installation](wiki/Installation.md) du wiki.

## Mettre à jour

- **Depuis le bureau** : Paramètres › Mises à jour, puis « Rechercher des mises à jour » et « Installer ». Les mises à jour automatiques peuvent être activées sur la même page.
- **En ligne de commande** :

```sh
winlux-update --check      # une nouvelle version est-elle publiée ?
winlux-update --apply      # télécharger, vérifier et installer
winlux-update --rollback   # revenir à la version précédente
winlux-update --auto enable
```

Winlux interroge uniquement la **dernière release publique** du dépôt GitHub (jamais les commits de `master`, jamais les préversions), sans jeton ni compte. Depuis la base APT, la mise à jour est un paquet Debian `winlux-desktop` installé par APT, avec vérification de la taille et du SHA-256, sauvegarde de la version précédente et retour arrière. Les applications et documents de l'utilisateur sont conservés. Voir la page [Publication et mises à jour](wiki/Publication-et-mises-a-jour.md).

## Utiliser seulement le runtime sur un Linux existant

Le runtime peut aussi servir seul, sur une distribution Linux x86-64 existante, sans installer tout le système :

```sh
git clone https://github.com/Univers4craft/winlux.git
cd winlux
cargo build --release -p winlux
target/release/winlux chemin/vers/programme.exe [arguments…]

# Optionnel : double-clic sur les .exe depuis le bureau Linux
sudo integration/install.sh      # installe winlux, winlux-open, binfmt, MIME
sudo integration/uninstall.sh    # retire cette intégration
```

Un serveur X11 est nécessaire pour les programmes graphiques. Variables utiles :

| Variable | Effet |
|---|---|
| `WINLUX_LOG=debug` (ou `trace`, `json`) | Niveau et format du journal. |
| `WINLUX_TRACE=1` | Trace complète du chargeur (imports résolus, relocations). |
| `WINLUX_MISSING=ignore` | Continue malgré un import non implémenté (renvoie 0, à utiliser pour diagnostiquer seulement). |
| `WINLUX_GUI=headless` | Backend graphique sans affichage, utilisé par les tests. |

L'intégration à GNOME et KDE est préparée mais n'a pas été validée sur ces bureaux.

## Compiler et tester

Prérequis : Linux x86-64, Rust stable (la CI utilise la dernière version stable), la cible `x86_64-pc-windows-msvc` pour les PE de test, et MinGW-w64 pour les tests C.

```sh
rustup target add x86_64-pc-windows-msvc

cargo build --release -p winlux          # le runtime : target/release/winlux
cargo test -p winlux                     # tests unitaires
bash scripts/build-tests.sh              # compile les PE de test (testexe/)
bash scripts/test-runtime.sh             # exécute chaque PE et vérifie sa sortie
bash scripts/test-runtime-parallel.sh    # les mêmes PE en parallèle
sh scripts/build-mingw-tests.sh && sh scripts/test-mingw.sh   # suites C MinGW

cargo fmt --check -p winlux              # exigé par la CI
cargo clippy -p winlux -- -D warnings    # exigé par la CI
```

La construction de l'ISO demande un hôte **Ubuntu 26.04** avec KVM : elle prépare la base Ubuntu officielle dans une VM jetable, puis assemble l'image. Elle est normalement faite par le workflow de publication. Les commandes, variables et pièges connus sont dans [docs/BUILD.md](docs/BUILD.md), [docs/NATIVE-APT.md](docs/NATIVE-APT.md) et la page [Construire l'ISO](wiki/Construire-l-ISO.md) du wiki.

### Intégration continue

| Workflow | Quand | Ce qu'il vérifie |
|---|---|---|
| [`ci.yml`](.github/workflows/ci.yml) | Chaque push et pull request | **Hygiène** (rustfmt, Clippy sans avertissement, aucune dépendance à Wine, scripts en LF et exécutables, chaque PE de test listé) ; **Tests unitaires** (runtime, installateur, verrouillage, UAC, autorisations, codecs réels, FFmpeg, mises à jour, réseau, partage Samba face à des clients Windows 11 et macOS simulés…) ; **Tests runtime** (PE Rust, suites C MinGW et MSVC) ; **Captures d'écran** sous Xvfb comparées à la dernière version verte. |
| [`apt-base.yml`](.github/workflows/apt-base.yml) | PR qui touche la recette de la base Ubuntu | Reconstruit la base APT comme la publication, sans rien publier. |
| [`release.yml`](.github/workflows/release.yml) | Tag `vX.Y.Z` | Construit l'ISO, la teste en VM (Live, navigateurs, bureau natif, installation BIOS/UEFI, mise à jour depuis la version précédente), puis publie l'ISO, le paquet `.deb`, le manifeste et les sommes SHA-256. |

Une CI verte ne garantit pas que tous les logiciels Windows fonctionnent : elle garantit que rien de ce qui marchait n'a cassé. Le détail est dans la page [Tests et CI](wiki/Tests-et-CI.md).

## Organisation du dépôt

| Dossier | Contenu |
|---|---|
| `runtime/` | Le runtime Winlux en Rust (crate `winlux`) : chargeur PE (`pe.rs`, `loader.rs`), DLL Windows (`dlls/`), CLR .NET (`clr/`), registre (`registry/`), chemins (`paths.rs`), msiexec (`msi/`), installateurs NSIS/Inno (`setup/`), archives (`archive/`), images (`imaging/`), son (`audio/`), langues et claviers (`intl/`), lecteurs réseau (`netdrive/`), bureau à distance (`remote/`), alimentation (`power/`), volumes, pare-feu, UAC, autorisations. |
| `testexe/` | Programmes Windows écrits en Rust `no_std`, compilés en vrais `.exe` : PE de test **et** applications maison du shell (`src/shell/` : Explorateur, Gestionnaire des tâches, Éditeur du Registre, Store…). |
| `testc/`, `testdll/` | Suites C/C++ compilées par MinGW et clang-cl (MSVC), DLL de test pour le chargement dynamique. |
| `scripts/` | Session de bureau native (menu Démarrer, Paramètres, verrouillage, UAC…), installateur, construction de l'ISO, outils système (`winlux-control`, `winlux-update`…) et tous les tests. |
| `integration/` | Lanceurs (`winlux-open`, `winlux-terminal`, `winlux-browser`), associations MIME, binfmt, configuration de la session XFCE, catalogue du Store. |
| `assets/` | Logo Prisme, thème GRUB, fonds d'écran, avatars. |
| `theme/` | Thème GTK de Winlux. |
| `docs/` | Documentation technique par sujet et notes de version (`docs/releases/`). |
| `wiki/` | Les pages du wiki, publiées sur le wiki GitHub du dépôt. |
| `Suivi.md` | **Source unique de vérité** des tâches faites et à faire (`TODO.md` et `TRACKING.md` en sont des miroirs). |

## Documentation

- **[Le wiki](wiki/Home.md)** : la documentation complète et organisée (but, architecture, compatibilité, registre, chemins et droits, réseau et disques, Paramètres, installation, publication, construction, tests, feuille de route).
- **Référence technique** dans `docs/` : [BUILD](docs/BUILD.md), [REGISTRY](docs/REGISTRY.md), [CHEMINS-ET-DROITS](docs/CHEMINS-ET-DROITS.md), [VOLUMES](docs/VOLUMES.md), [PARTAGE](docs/PARTAGE.md), [NATIVE-DESKTOP](docs/NATIVE-DESKTOP.md), [NATIVE-APT](docs/NATIVE-APT.md), [SECURE-BOOT](docs/SECURE-BOOT.md), [RECOVERY](docs/RECOVERY.md), [MEDIA](docs/MEDIA.md), [AUDIO](docs/AUDIO.md), [INTL](docs/INTL.md), [STORE](docs/STORE.md), [MSIEXEC](docs/MSIEXEC.md), [SETUP-EXE](docs/SETUP-EXE.md), [PAINTNET](docs/PAINTNET.md), [IDENTITE-VISUELLE](docs/IDENTITE-VISUELLE.md), [AB_UPDATES](docs/AB_UPDATES.md)…
- **Notes de version** : [docs/releases/](docs/releases/).
- **Interface** : [INTERFACE.md](INTERFACE.md). **Compatibilité** : [COMPATIBILITY.md](COMPATIBILITY.md).

## Contribuer et signaler un problème

- **Signaler un bug depuis Winlux** : Paramètres › Signaler un problème, ou après un plantage, prépare un rapport (version, matériel, journal) avec les données personnelles masquées, puis l'ouvre sur GitHub.
- **À la main** : ouvrez une [issue](https://github.com/Univers4craft/winlux/issues) en indiquant la version (`/usr/share/winlux/BUILD-INFO` ou Paramètres › À propos), le logiciel concerné, les étapes pour reproduire et le journal (`WINLUX_LOG=debug winlux programme.exe`).
- **Contribuer du code** : lisez [AGENTS.md](AGENTS.md) et la page [Comment contribuer](wiki/Comment-contribuer.md). En résumé : une branche par sujet, `cargo fmt` et `cargo clippy -D warnings` propres, tests ajoutés, `Suivi.md` mis à jour dans le même commit, aucune dépendance à Wine, aucun élément de marque Microsoft, et toute nouvelle chaîne visible traduite dans les quatre langues.

## Licence

Winlux est distribué sous [GNU GPL version 3 ou ultérieure](LICENSE). Le logo Prisme, les fonds d'écran, les sons et le thème sont des créations du projet sous la même licence. Les logiciels Linux embarqués dans l'ISO (noyau, paquets Ubuntu, FFmpeg, Samba…) gardent leurs propres licences, listées dans [NOTICES](NOTICES).

Winlux n'est ni affilié à Microsoft, ni approuvé par Microsoft. « Windows » est une marque de Microsoft Corporation, citée uniquement pour décrire la compatibilité.
