# Winlux

[![CI](https://github.com/Univers4craft/winlux/actions/workflows/ci.yml/badge.svg)](https://github.com/Univers4craft/winlux/actions/workflows/ci.yml)

Winlux est un runtime expérimental Linux qui charge des exécutables **Windows PE32+ x86-64** et fournit un sous-ensemble d’API Win32 écrit en Rust. Le projet vise une interopérabilité progressive avec Linux; il ne s’appuie pas sur Wine.

> **État expérimental :** la compatibilité est partielle. Ce projet ne garantit ni l’exécution de tout logiciel Windows, ni l’absence de crash. Les applications GUI complexes et les interfaces .NET Windows restent incomplètes.

## État testé

Les 70 tests unitaires du runtime et neuf exécutables PE couvrent le chargeur et les principales API implémentées. Les exécutables incluent `hello.exe`, `exitcode.exe`, `p1_suite.exe`, des tests de DLL réelle/import différé, `findfiles.exe`, `crt_suite.exe` (chaînes, conversions, mathématiques, tri) et deux tests GUI (`gui_suite.exe` pour le cycle de vie et le redimensionnement, `gui_demo.exe` pour le tracé GDI et le rendu X11). Ils sont compilés en PE Windows x64 avec Rust et `rust-lld`, puis exécutés directement sous Winlux, séquentiellement ou tous en parallèle.

Le runtime contient notamment un chargeur PE, les relocations et imports, un PEB/TEB minimal, du TLS, des DLL intégrées partielles, le registre Winlux persistant, des fichiers mappés vers Linux (énumération, fichiers temporaires, métadonnées), une fondation COM/OLE native (`CoInitializeEx`, GUID, mémoire COM et `IStream` sur `HGLOBAL`) et une passerelle Winsock/ALSA expérimentale.

Le sous-ensemble de processus implémente `CreateProcessA/W` pour lancer un programme Linux ou un PE enfant via Winlux, avec attente, code de sortie, blocs d’environnement A/W et `CREATE_SUSPENDED`/`ResumeThread`. L’héritage des handles et les attributs étendus ne sont pas encore implémentés. Les fichiers prennent en charge des E/S overlapped élémentaires via `ReadFile`/`WriteFile`, `GetOverlappedResult`, une annulation best-effort avec `CancelIo/CancelIoEx` et un sous-ensemble IOCP.

Le prototype de bureau X11 possède maintenant un thème professionnel HiDPI : papier peint vectoriel, barre des tâches pleine largeur, indicateurs système à gauche et menu Démarrer inspiré de Cinnamon avec profil, emplacements, catégories, recherche et accès aux paramètres Linux/Windows. Les applications utilisent leurs vraies icônes PNG déclarées par les fichiers `.desktop` et les thèmes XDG; le runtime les met en cache, conserve leur transparence et les redimensionne par interpolation bilinéaire. Le même rendu est conçu pour 1920×1080 à 150 %, 2560×1440 à 200 % et 3840×2160 à 300 % avec du texte Xft antialiasé.

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
```

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
- `integration/` : fichiers préparatoires binfmt, MIME, `.desktop` et script d’installation.
- `scripts/` : compilation, tests locaux séquentiels/parallèles et pipeline ISO/QEMU.
- `theme/` : palette CSS expérimentale, non raccordée à un shell graphique.
- `TODO.md` : feuille de route maintenue.
- `COMPATIBILITY.md` : état détaillé de compatibilité et limites connues.

## Intégration Linux et ISO

La configuration binfmt et le fichier `.desktop` sont des préparatifs, pas une intégration hôte validée. `integration/install.sh` modifie des emplacements système et nécessite une revue ainsi que des privilèges d’administration pour être exécuté.

`scripts/build-live-iso.sh` construit une image de test autonome avec un initramfs minimal, Winlux, les neuf PE, leur DLL, Xvfb et un démarrage GRUB BIOS/UEFI. Le noyau fourni doit être lisible par l’utilisateur :

```sh
WINLUX_KERNEL=/chemin/vers/vmlinuz scripts/build-live-iso.sh
scripts/test-live-iso-qemu.sh build/winlux-live-test.iso bios
scripts/test-live-iso-qemu.sh build/winlux-live-test.iso uefi
```

La VM exécute la suite séquentielle, les neuf PE simultanément et le test graphique X11. Le test QEMU exige 1 Gio de RAM et 2 CPU par défaut, configurables avec `WINLUX_QEMU_MEMORY` et `WINLUX_QEMU_CPUS`. L’ISO est une image de validation automatisée, pas encore un bureau Winlux destiné aux utilisateurs.

## Limites importantes

- PE32+ x86-64 seulement; le runtime n’est pas un noyau Windows.
- `user32`/`gdi32` sont partiels : le noyau fenêtres/messages, les boucles de messages A/W, les titres Unicode, `SetWindowPos`/`MoveWindow`, les API de focus et d'état clavier de base, un backend X11 avec mise à l'échelle DPI, texte Xft antialiasé et bitmaps filtrés, ainsi que des primitives GDI de base existent, mais les contrôles, les autres formats DIB, les décorations et la plupart des API clavier avancées manquent encore.
- Le pont `dotnet`/Mono permet d'essayer les applications managées compatibles Linux, mais aucun CLR Windows complet n'est intégré. WPF/WinForms et `paint.net` ne fonctionnent donc pas encore correctement.
- COM/OLE possède désormais ses primitives de base et ses flux mémoire, mais l’activation des classes COM, l’automatisation, le glisser-déposer et les serveurs COM applicatifs restent à implémenter.
- Les codecs compressés, DirectX, les pilotes, le registre Windows complet, les services SCM complets, les sémantiques avancées des E/S overlapped et de nombreuses API Win32 ne sont pas implémentés.
- `FreeLibrary` ne démonte pas encore une DLL chargée.
- Le pont audio WAV/ALSA dépend de l’environnement Linux et n’a pas été validé sur plusieurs périphériques.

Voir [COMPATIBILITY.md](COMPATIBILITY.md) pour les détails et [TODO.md](TODO.md) pour les travaux restants.

## Licence

Winlux est distribué sous GNU GPL version 3 ou, à votre choix, toute version ultérieure. Voir [LICENSE](LICENSE).
