# Winlux

[![CI](https://github.com/Univers4craft/winlux/actions/workflows/ci.yml/badge.svg)](https://github.com/Univers4craft/winlux/actions/workflows/ci.yml)

Winlux est un runtime expérimental Linux qui charge des exécutables **Windows PE32+ x86-64** et fournit un sous-ensemble d’API Win32 écrit en Rust. Le projet vise une interopérabilité progressive avec Linux; il ne s’appuie pas sur Wine.

> **État expérimental :** la compatibilité est partielle. Ce projet ne garantit ni l’exécution de tout logiciel Windows, ni l’absence de crash. Les applications GUI complexes et les interfaces .NET Windows restent incomplètes.

## État testé

Les tests unitaires du runtime et dix exécutables PE couvrent le chargeur et les principales API implémentées. Les exécutables incluent `hello.exe`, `exitcode.exe`, `p1_suite.exe`, des tests de DLL réelle/import différé, `findfiles.exe`, `crt_suite.exe`, les tests GUI et `explorer_regression.exe` (noms longs, copies, chemins et raccourcis). Ils sont compilés en PE Windows x64 avec Rust et `rust-lld`, puis exécutés directement sous Winlux, séquentiellement ou tous en parallèle.

Le runtime contient notamment un chargeur PE, les relocations et imports, un PEB/TEB minimal, du TLS, des DLL intégrées partielles, le registre Winlux persistant, des fichiers mappés vers Linux (énumération, fichiers temporaires, métadonnées), une fondation COM/OLE native (`CoInitializeEx`, GUID, mémoire COM et `IStream` sur `HGLOBAL`) et une passerelle Winsock/ALSA expérimentale.

Le sous-ensemble de processus implémente `CreateProcessA/W` pour lancer un programme Linux ou un PE enfant via Winlux, avec attente, code de sortie, blocs d’environnement A/W et `CREATE_SUSPENDED`/`ResumeThread`. L’héritage des handles et les attributs étendus ne sont pas encore implémentés. Les fichiers prennent en charge des E/S overlapped élémentaires via `ReadFile`/`WriteFile`, `GetOverlappedResult`, une annulation best-effort avec `CancelIo/CancelIoEx` et un sous-ensemble IOCP.

Le prototype de bureau X11 possède maintenant un thème professionnel HiDPI : papier peint vectoriel animé avec accents violets, barre des tâches compacte pleine largeur, bouton Démarrer réduit à son icône, indicateurs réseau/audio/batterie dynamiques à droite et menu inspiré de Cinnamon plus compact, avec profil, emplacements, catégories, recherche et accès aux paramètres Linux/Windows. Les raccourcis du bureau peuvent être alignés sur une grille ou déplacés librement, y compris en se superposant. Les applications utilisent leurs vraies icônes PNG déclarées par les fichiers `.desktop` et les thèmes XDG; le runtime les met en cache, conserve leur transparence et les redimensionne par interpolation bilinéaire. Le même rendu est conçu pour 1920×1080 à 150 %, 2560×1440 à 200 % et 3840×2160 à 300 % avec du texte Xft antialiasé.

## Prérequis

- Linux x86-64
- Rust stable et Cargo
- Cible Rust `x86_64-pc-windows-msvc` et `rust-lld` pour compiler les exécutables de test
- `timeout` pour `scripts/test-runtime.sh`
- ALSA (`libasound.so.2`) seulement pour tenter la lecture audio WAVE

Installer la cible de test, si nécessaire :

```sh
rustup target add x86_64-pc-windows-msvc
```

## Compiler et tester

Depuis la racine du dépôt :

```sh
cargo test -p winlux
scripts/build-tests.sh
cargo build --release -p winlux
scripts/test-runtime.sh
scripts/test-runtime-parallel.sh
scripts/test-network.sh
scripts/test-control.sh
scripts/test-updater.sh
scripts/build-mingw-tests.sh   # nécessite gcc-mingw-w64-x86-64
scripts/test-mingw.sh
```

`testc/` contient des programmes C compilés par MinGW-w64, donc de vrais exécutables Windows produits par un compilateur tiers et non par Rust : ils valident le code de démarrage de la CRT `msvcrt.dll` et les API qu'il utilise.

Lancement manuel d’un PE :

```sh
target/release/winlux chemin/vers/programme.exe [arguments...]
```

`WINLUX_TRACE=1` active les traces du chargeur. Par défaut, l’appel d’un import non implémenté arrête le programme et l’enregistre dans le journal Winlux. `WINLUX_MISSING=ignore` permet de continuer en renvoyant zéro, avec les risques de résultats incorrects.

Winlux détecte aussi les en-têtes CLR PE32/PE32+ et les apphosts .NET modernes.
Il délègue les assemblies modernes au pilote Linux `dotnet` et les anciens
assemblages .NET Framework à `mono`. Les chemins peuvent être surchargés avec
`WINLUX_DOTNET` et `WINLUX_MONO`. Ce pont convient aux applications managées
multiplateformes ; il ne rend pas encore WPF ou WinForms compatibles.

## Arborescence

- `runtime/` : runtime Linux et chargeur PE.
- `testexe/` : exécutables Windows de test.
- `testdll/` : DLL Windows de test.
- `integration/` : binfmt, MIME, `.desktop`, lanceur de double-clic et scripts d’installation/désinstallation.
- `scripts/` : compilation, tests locaux séquentiels/parallèles et pipeline ISO/QEMU.
- `theme/` : palette CSS expérimentale, non raccordée à un shell graphique.
- `TODO.md` : feuille de route maintenue.
- `COMPATIBILITY.md` : état détaillé de compatibilité et limites connues.
- `INTERFACE.md` : état du prototype graphique et exigences de l'interface cible.
- [Revue d’intégration des PR](REVIEW-INTEGRATION.md) : ajouts retenus et parties reportées après examen.

## Intégration Linux et ISO

Chaque nouvelle ISO et mise à jour fournit `/usr/share/winlux/BUILD-INFO` :
version, commit Git, date de construction UTC et périmètre système/runtime.
Le paquet système conserve les métadonnées de son ISO. `tracked_sources=modified`
signale des changements suivis non validés ; sans dépôt Git, le commit est `unknown`.
La date concerne la construction du livrable, pas nécessairement la compilation
d'un binaire réutilisé. `SOURCE_DATE_EPOCH` permet de fixer cette date.

`integration/install.sh` installe le runtime, le lanceur de double-clic `winlux-open`, l’outil de mise à jour, l’enregistrement binfmt et l’association MIME des `.exe` (par défaut dans `/usr`; il modifie des emplacements système et nécessite des privilèges d’administration) :

```sh
cargo build --release -p winlux
sudo integration/install.sh          # ou /usr/local
sudo integration/uninstall.sh        # retire tout et restaure les associations précédentes
```

Au double-clic, `winlux-open` lit le sous-système du PE (`winlux --inspect`) : un programme console s’ouvre dans un terminal qui reste ouvert à la fin, un programme graphique est lancé directement depuis son dossier. Les erreurs (fichier illisible, programme 32 bits, DLL…) s’affichent dans une boîte de dialogue. `WINLUX_TERMINAL` force un émulateur de terminal précis. `scripts/test-integration.sh` valide l’installation dans une racine temporaire et le lanceur; l’installation réelle sur un bureau KDE, GNOME ou XFCE reste à valider.

`scripts/build-live-iso.sh` construit une image de test autonome avec un initramfs minimal, Winlux, les neuf PE, leur DLL, Xvfb et un démarrage GRUB BIOS/UEFI. Le noyau fourni doit être lisible par l’utilisateur :

```sh
WINLUX_KERNEL=/chemin/vers/vmlinuz scripts/build-live-iso.sh
scripts/test-live-iso-qemu.sh build/winlux-live-test.iso bios
scripts/test-live-iso-qemu.sh build/winlux-live-test.iso uefi
```

La VM automatisée exécute la suite séquentielle, les neuf PE simultanément et le test graphique X11. Le test QEMU utilise 2 Gio de RAM et 2 CPU par défaut, configurables avec `WINLUX_QEMU_MEMORY` et `WINLUX_QEMU_CPUS`; cette marge évite l’échec de décompression de l’initramfs enrichi sous UEFI.

Une VM graphique persistante de prévisualisation peut aussi être créée puis démarrée avec :

```sh
scripts/create-test-vm.sh --create-only
scripts/create-test-vm.sh
# Après reconstruction de l'ISO, réinitialiser explicitement le disque :
scripts/create-test-vm.sh --refresh  # conserve l'ancien disque en sauvegarde
```

Elle utilise par défaut un disque QCOW2 extensible de 24 Gio, 4 Gio de RAM, 4 CPU, KVM lorsqu’il est disponible et un affichage 1920×1080. La définition peut être changée avec `WINLUX_VM_WIDTH` et `WINLUX_VM_HEIGHT`. Une empreinte des sources empêche d'utiliser par erreur un stage ou un disque ancien. `--refresh` crée une nouvelle racine et conserve l'ancien disque complet ; il ne migre pas automatiquement ses données. Pour conserver la racine courante, utilisez les mises à jour dans l'OS.

Dans le gestionnaire de VM, démarrez sur l'ISO avec un disque virtio portant le label `WINLUX_ROOT` : GRUB charge alors le noyau et la racine depuis ce disque. Les données, préférences et mises à jour survivent au redémarrage. Sans ce disque, l'ISO démarre une session Live en mémoire, signalée dans Paramètres. L'image reste une préversion technique sans installateur généraliste. La résolution du bureau suit la fenêtre QEMU et le curseur logiciel Xorg reste visible. Ajoutez `winlux.debug=1` au noyau pour les traces d'entrée. La tablette virtio utilise des coordonnées absolues et évite la capture relative du pointeur.

Le démarrage configure les cartes virtuelles par DHCP (adresse, route et DNS), avec renouvellement du bail. `winlux-network restart` renouvelle la connexion ; le journal se trouve dans `/run/winlux/network.log`. Les dispositions AZERTY/QWERTY et les options libinput sont disponibles dans Paramètres. Un trackpad de l'hôte présenté comme tablette par l'hyperviseur conserve ses réglages physiques sur l'hôte.

### Bureau, Explorer et navigateur

Le bureau de l’ISO est un shell maison au style Windows 11 (`testexe/src/shell/`) : animation de démarrage, menu Démarrer et barre des tâches animés, fenêtres déplaçables, redimensionnables et ancrables (glisser en haut pour agrandir, sur un bord pour la moitié de l’écran). **Explorer** y est le gestionnaire de fichiers : onglets, double volet, modes Très grandes / Grandes / Moyennes / Petites icônes, Liste, Détails, Mosaïques et mode compact, tri, recherche récursive, barre d’adresse qui accepte les chemins Windows et Linux, couper / copier / coller / renommer, corbeille freedesktop. C: est le dossier personnel et Z: la racine Linux.

Le navigateur par défaut est **Microsoft Edge**, lancé par `winlux-browser` (repli sur Chromium ou Firefox s’il est absent). Sur un système installé : `sudo /usr/share/winlux/install-edge.sh` ajoute le dépôt officiel Microsoft et fait d’Edge le navigateur par défaut. Pour l’embarquer dans l’ISO : `WINLUX_EDGE_DEB=/chemin/microsoft-edge-stable.deb scripts/build-live-iso.sh`.

Paramètres propose les pages Linux, compatibilité Windows, réseau, audio/batterie, clavier/trackpad, apparence et mises à jour. Les commandes passent par `winlux-control`, avec télémétrie réelle, exécution en arrière-plan et journal visible. Les applications non installées ne possèdent pas de bouton fictif. Explorer bloque les copies dans un descendant et les chemins tronqués ; les noms Unicode hors Windows-1252 restent à prendre en charge avec les API fichiers larges.

La page Écran et définition utilise `winlux-display` et les modes XRandR réels : définition fixe ou adaptation au mode préféré de la VM, plus échelle automatique/100/125/150/200 % (relance du bureau pour l'échelle). Les préférences sont dans `/etc/winlux/display.conf`. Cela ne modifie pas le zoom du visualiseur sur l'hôte.

## Mises à jour GitHub

Le système ne se met pas à jour depuis les commits : il lit le manifeste de la dernière **release publiée**. Les tags stables `v*` déclenchent le workflow runtime. Les tags `v*-preview.*` sont exclus de ce workflow pour ne pas écraser une release système complète avec un paquet réduit. La version système de référence se trouve dans `VERSION`. Sur une installation Winlux :

```sh
winlux-update --check
winlux-update --list
winlux-update --apply
winlux-update --rollback
winlux-update --auto enable  # désactiver avec disable
```

Pour un dépôt GitHub privé, fournissez un jeton en lecture dans `WINLUX_UPDATE_TOKEN` (ou `GH_TOKEN`). L’outil passe alors par l’API GitHub authentifiée et ne journalise pas le jeton. Pour un dépôt public, aucune authentification n’est nécessaire.

Le paquet `scripts/build-update-bundle.sh --system VERSION` comprend le bureau, le runtime, l'outil de mise à jour, les applications embarquées, les scripts, le noyau, les pilotes, les bibliothèques et le démarrage. Il exige un stage ISO correspondant exactement aux sources. `--runtime` produit un paquet réduit, utilisé par la CI lorsqu'aucune image système n'a été construite. Le manifeste affiche clairement cette portée dans l'interface.

Les modifications détaillées proviennent de `docs/releases/VERSION.md` (ou de `WINLUX_UPDATE_NOTES`). Le constructeur produit `winlux-release-notes.txt`, à joindre à la même release, avec son SHA-256 dans le manifeste. Après Rechercher, le bas de la page affiche ces notes, conservées hors ligne et lisibles avec retour à la ligne, molette et touches de navigation. Le journal technique reste accessible séparément. Une ancienne release sans notes est signalée explicitement.

Pour publier une préversion système : construire l'ISO avec `scripts/build-live-iso.sh`, exécuter les tests QEMU, puis `scripts/build-update-bundle.sh --system` (même `VERSION`). Publier sur le tag correspondant l'ISO, sa somme SHA-256, `winlux-update-VERSION.tar.gz`, `winlux-update-manifest.txt` et `winlux-release-notes.txt` lorsqu'il est indiqué par le manifeste. La release doit être désignée comme dernière release (`--latest`), non brouillon et non marquée « prerelease », car l'API `/releases/latest` exclut les prereleases ; son titre et ses notes doivent préciser qu'il s'agit d'une préversion expérimentale. Ne jamais inclure de jeton dans ces fichiers.

L’outil valide les manifestes, SHA-256, inventaires et liens, verrouille les opérations concurrentes, conserve les fichiers précédents et restaure une installation interrompue. Les données personnelles et `/etc` sont préservés ; les nouveaux réglages par défaut sont consultables dans `/usr/share/winlux/defaults/etc`. Un changement du bureau seul propose une relance de la session ; un changement du noyau ou des composants système demande un redémarrage explicite. L'automatique vérifie toutes les trente minutes sans redémarrer la machine sans action de l'utilisateur. Cette mise à jour transactionnelle n'est pas encore un schéma A/B résistant à toute coupure d'alimentation.

L'installation affiche ses phases et le fichier en cours dans `/run/winlux/update-progress`. `--busy` identifie l'installateur actif ; les commandes d'alimentation Winlux et l'init refusent alors l'arrêt/redémarrage. Le marqueur persistant `/var/lib/winlux/pending-reboot` est acquitté par `winlux-update --boot` uniquement après un vrai changement de boot ID. Ce contrôle au démarrage restaure aussi une transaction interrompue avant de démarrer les services. Une coupure forcée de l'hyperviseur reste impossible à interdire depuis l'OS invité et n'est pas garantie sans risque.

## Limites importantes

- PE32+ x86-64 seulement; le runtime n’est pas un noyau Windows.
- `user32`/`gdi32` sont partiels : le noyau fenêtres/messages, les boucles de messages A/W, les titres Unicode, `SetWindowPos`/`MoveWindow`, les API de focus et d'état clavier de base, un backend X11 avec mise à l'échelle DPI, texte Xft antialiasé et bitmaps filtrés, ainsi que des primitives GDI de base existent, mais les contrôles, les autres formats DIB, les décorations et la plupart des API clavier avancées manquent encore.
- Le pont `dotnet`/Mono permet d'essayer les applications managées compatibles Linux, mais aucun CLR Windows complet n'est intégré. WPF/WinForms et `paint.net` ne fonctionnent donc pas encore correctement.
- COM/OLE possède désormais ses primitives de base et ses flux mémoire, mais l’activation des classes COM, l’automatisation, le glisser-déposer et les serveurs COM applicatifs restent à implémenter.
- Les codecs compressés, DirectX, les pilotes, le registre Windows complet, les services SCM complets, les sémantiques avancées des E/S overlapped et de nombreuses API Win32 ne sont pas implémentés.
- `FreeLibrary` ne démonte pas encore une DLL chargée.
- Le pont audio WAV/ALSA dépend de l’environnement Linux et n’a pas été validé sur plusieurs périphériques.

Voir [COMPATIBILITY.md](COMPATIBILITY.md) pour les détails de compatibilité,
[INTERFACE.md](INTERFACE.md) pour les exigences du bureau et
[TODO.md](TODO.md) pour la feuille de route générale. La compatibilité .NET
Windows, notamment WinForms et WPF, reste un objectif à construire par étapes ;
le pont Linux actuel ne la fournit pas.

## Licence

Winlux est distribué sous GNU GPL version 3 ou, à votre choix, toute version ultérieure. Voir [LICENSE](LICENSE).
