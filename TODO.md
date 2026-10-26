# Feuille de route Winlux

La feuille de route détaillée pour l'installation sur SSD et l'usage quotidien
se trouve dans [ROADMAP-SSD.md](ROADMAP-SSD.md).

Audit du 25 octobre 2026 : voir [les preuves et réserves](docs/AUDIT-MARKDOWN.md).
Une case cochée concerne uniquement le périmètre décrit ; les validations VM
ne valent pas validation sur matériel réel. Les tâches partielles sont séparées.

## État actuel

### Fiabilisation du bureau et de la VM

- [x] Préserver les noms longs et refuser les chemins tronqués dans Explorer.
- [x] Bloquer les copies dans un descendant, les collisions et les alias de chemins ; tester le couper/coller et les raccourcis.
- [x] Corriger Ctrl+L, les touches Windows, AltGr et la disposition XKB réelle.
- [x] Détecter les images/disques périmés par empreinte des sources et conserver le disque précédent lors d'un rafraîchissement.
- [x] Charger le noyau et la racine persistante du disque depuis GRUB.
- [x] Ajouter DHCP, route, DNS et renouvellement réseau, avec tests isolés.
- [x] Paramètres Linux/Windows, télémétrie réelle et commandes système avec journaux.
- [x] Centre graphique de mise à jour, paquets système complets, jeton pour dépôt privé, automatique, restauration et demande de redémarrage.
- [x] Ajouter un installateur autonome BIOS/UEFI qui ne nécessite plus l'ISO comme chargeur initial.
- [x] Passer Explorer entièrement aux chemins Unicode Win32 W.
- [x] Exécuter le bureau de l'ISO en utilisateur non privilégié avec autorisation ciblée des actions système (le script Xsession existe, mais l'init lance encore le bureau en root).
- [ ] Signer les mises à jour et utiliser des racines A/B pour les coupures d'alimentation (voir `docs/AB_UPDATES.md`).

- Validation de `v0.2.0-preview.2` : `cargo test -p winlux` (84 réussis, 1 test XKB ignoré sans serveur X), dix exécutables PE en séquentiel et en parallèle, tests réseau/paramètres/mise à jour-restauration, et ISO sous QEMU BIOS et UEFI réussis.
- [x] Charger un exécutable PE32+ x86-64.
- [x] Appliquer les relocations et résoudre les imports de base.
- [x] Initialiser un PEB/TEB minimal via `GS`.
- [x] Fournir les API nécessaires aux tests P1.
- [x] Implémenter les ponts `printf`/`sprintf` variadiques Windows x64.
- [x] Exécuter `hello.exe`, `exitcode.exe`, `p1_suite.exe`, `dll_import.exe`, `delay_import.exe`, `findfiles.exe`, `crt_suite.exe`, `gui_suite.exe` et `gui_demo.exe` sous Winlux.
- [x] Ajouter des tests unitaires couvrant le runtime, le registre, le SCM, l'audio, Winsock, les processus, les fichiers, la CRT, IOCP, COM/OLE et le noyau fenêtres/messages.
- [x] Exécuter les dix PE simultanément avec isolation, temporisation et journaux par processus.
- [x] Ajouter les fichiers binfmt, MIME et `.desktop`.
- [x] Ajouter un premier script de génération ISO.
- [x] Ajouter une base de thème visuel inspirée de Windows 11.

## Priorité 1 — Fiabiliser le runtime

- [ ] Continuer à remplacer les implémentations partielles de `kernel32`, `msvcrt` et `ntdll`; ces modules ne constituent pas des DLL Windows complètes.
- [x] Ajouter des tests unitaires pour le parsing PE et le quoting de la ligne de commande.
- [x] Ajouter un test d’intégration automatisé des trois exécutables de référence.
- [x] Corriger la gestion de `GetTickCount` et utiliser une horloge monotone globale.
- [x] Améliorer la gestion de `VirtualAlloc`/`VirtualFree`, dont l’alignement 64 Ko.
- [x] Isoler les valeurs TLS Win32 par thread.
- [x] Valider les indices TLS, les libérer avec `TlsFree` et invalider les valeurs obsolètes.
- [x] Implémenter `VirtualProtect`, `MEM_DECOMMIT` et le recommit de mémoire.
- [x] Ajouter un test dédié aux exports par nom et par ordinal.
- [x] Déclencher les callbacks TLS et `DllMain` lors de la création de threads Win32.
- [x] Ajouter la validation des références d’imports différés et leur conversion RVA/VA.
- [x] Ajouter un test d’intégration avec une DLL réelle.
- [x] Ajouter le chargement dynamique via `LoadLibraryA/W`, `GetProcAddress` par nom/ordinal et les callbacks d’initialisation.
- [x] Ajouter un test synthétique exerçant effectivement la résolution d’un import différé.
- [x] Ajouter un binaire Windows compilé avec un vrai répertoire d’imports différés.
- [x] Implémenter une pile de secours réellement sûre pour les débordements de pile; le gestionnaire de signal est sans allocation/verrou, mais la protection contre le débordement n’est pas validée.
- [x] Éviter les allocations et verrous dans le gestionnaire de signaux de crash.
- [x] Définir la politique pour les imports manquants : journalisation, arrêt par défaut et mode explicite `WINLUX_MISSING=ignore`.
- [x] Renforcer les vérifications de bornes des sections, imports et IAT avant accès mémoire.

## Priorité 2 — Compatibilité Windows

- [x] Ajouter des DLL intégrées courantes : `user32`, `advapi32`, `ole32`, `bcrypt`, `version` et `shell32`.
- [x] Ajouter la fondation COM/OLE native : appartements par thread, GUID, allocateur COM, mémoire globale et `IStream` partageable sur `HGLOBAL`, avec tests unitaires et PE.
- [x] Enregistrer/révoquer des fabriques COM multi-usages et créer leurs objets dans le processus, avec cookies stables et appels réentrants testés.
- [ ] Compléter COM/OLE : activation depuis le registre, serveurs hors processus, automatisation et objets OLE applicatifs.
- [x] Ajouter un diagnostic explicite pour les exécutables gérés dépendant de `mscoree.dll`.
- [x] Détecter les en-têtes CLR PE32/PE32+ et les apphosts modernes, puis relayer les assemblies vers `dotnet` ou Mono lorsqu'un runtime Linux compatible est installé.
- [x] Relier les API de base fichiers/répertoires Windows au système de fichiers Linux.
- [x] Ajouter des variantes Unicode `W` pour les chemins, fichiers et variables d’environnement.
- [x] Ajouter `GetFullPathName`, `CopyFile`, `MoveFile`, `CreateDirectory` et `RemoveDirectory` en A/W ou A selon l’API.
- [x] Ajouter `GetTempFileNameA/W`, `GetFileAttributesExA/W` et `SetEndOfFile` avec tests fonctionnels.
- [x] Ajouter l’énumération de répertoires `FindFirstFileA/W`, `FindNextFileA/W` et `FindClose` avec motifs `*`/`?` et tests PE.
- [x] Implémenter les conversions UTF-8 et Windows-1252 avec vérification des buffers.
- [x] Ajouter les API de base `GetSystemInfo`, version OS, horloge FILETIME et plusieurs primitives `Rtl*`.
- [x] Ajouter les premières API GUI (`user32`, `gdi32`) et réseau (`ws2_32`).
- [x] Construire le noyau fenêtres/messages (`gui.rs`) : classes, table HWND, file par thread, appel du `WndProc` invité et palette claire inspirée de Windows 11, avec un test PE `gui_suite.exe` (séquence WM_CREATE/WM_SIZE/WM_PAINT/WM_DESTROY).
- [x] Relier le noyau à un backend X11 réel (`gui_x11.rs`, x11rb) : vraies fenêtres, surfaces claires et accent bleu rendus côté serveur, événements X traduits en messages Win32 (`Expose`→`WM_PAINT`, `ConfigureNotify`→`WM_SIZE`, souris, clavier X11→`VK_*`/`WM_KEYDOWN`/`WM_KEYUP`, `WM_DELETE_WINDOW`→`WM_CLOSE`). `TranslateMessage` produit `WM_CHAR` (ASCII/Latin-1 et BMP) ; tracé GDI et saisie vérifiés par `gui_demo.exe` sous Xvfb. Backend *headless* conservé pour les tests (`WINLUX_GUI=headless`).
- [x] Implémenter `SetWindowPos` et `MoveWindow` : subset de flags `SWP_NOMOVE`/`SWP_NOSIZE`/`SWP_NOREDRAW`, rectangles HWND actualisés, messages `WM_MOVE`/`WM_SIZE`, configuration de la vraie fenêtre X11 et synchronisation des positions/tailles via `ConfigureNotify`; testé par `gui_suite.exe` en headless et sous Xvfb.
- [x] Ajouter le focus clavier par thread (`GetFocus`/`SetFocus`), les notifications synchrones `WM_SETFOCUS`/`WM_KILLFOCUS`, la perte de focus à la destruction et la traduction des événements X11 `FocusIn`/`FocusOut`; testé par `gui_suite.exe` et par injection de focus sous Xvfb.
- [x] Ajouter `GetKeyState`, `GetAsyncKeyState` et `GetKeyboardState` : état thread mis à jour lors du retrait de `WM_KEYDOWN`/`WM_KEYUP`, état asynchrone alimenté par les événements X11, bits pressé/bascule et tableau de 256 touches; tests unitaires, PE et saisie Xvfb.
- [x] Ajouter les variantes Unicode de la boucle de messages et des titres (`GetMessageW`, `PeekMessageW`, `DispatchMessageW`, `PostMessageW`, `SendMessageW`, `Set/GetWindowTextW`) avec tests PE.
- [x] Ajouter la couche de dessin GDI : commandes (`FillRect`, `FrameRect`, `Ellipse`, `Text`, `BitBlt`, `StretchBlt`) accumulées par cycle de peinture puis rejouées par le backend (y compris à chaque `Expose`), pinceaux/plumes/polices (`CreateSolidBrush`, `CreatePen`, `CreateFontA/W`, `SelectObject`, `DeleteObject`, `GetStockObject`, `GetObjectA/W`), `GetSysColor`/`GetSysColorBrush`, `PatBlt` (PATCOPY/BLACKNESS/WHITENESS/PATINVERT/PATAND/PATOR/DSTINVERT), `CreateCompatibleBitmap`/`CreateDIBSection` 32bpp et DC mémoire pour `SRCCOPY`, `SRCPAINT`, `SRCAND`, `SRCINVERT`, `NOTSRCCOPY`, couleurs et mode de fond (`SetTextColor`, `SetBkColor`, `SetBkMode`), `TextOutA`, `Rectangle`, `Ellipse` et `GetDC`/`ReleaseDC`. `gui_demo.exe` vérifie le tracé; rendu et blits contrôlés par capture X11 (`xwd`).
- [x] Relier sockets TCP/UDP, erreurs WSA et résolveur `getaddrinfo` A/W aux interfaces POSIX (tests loopback).
- [x] Ajouter un registre persistant partagé Linux/Windows.
- [x] Ajouter une couche SCM locale, non privilégiée, qui persiste les définitions et lance/arrête les services en processus utilisateur.
- [x] Implémenter `StartServiceCtrlDispatcher` et le canal de contrôle/notification des vrais services Windows.
- [x] Ajouter les premiers points d’entrée multimédia `mfplat` et `winmm`.
- [x] Ajouter le décodage WAVE PCM/float et un pont ALSA optionnel pour `PlaySoundA/W` (parseur testé; sortie physique encore à valider sur machine audio).
- [x] Ajouter les primitives de synchronisation `Event`, `Mutex` et `WaitForSingleObject`.
- [x] Ajouter `VirtualQuery`, `WaitForMultipleObjects` wait-any/wait-all et les sections critiques récursives.
- [x] Ajouter `ReadFile`/`WriteFile` overlapped avec offsets, `ERROR_IO_PENDING`, signal d’événement et `GetOverlappedResult`.
- [x] Ajouter une annulation best-effort des opérations overlapped en attente avec `CancelIo` et `CancelIoEx`.
- [x] Ajouter un sous-ensemble IOCP : association d’un fichier, `GetQueuedCompletionStatus[Ex]` et `PostQueuedCompletionStatus`.
- [x] Respecter le plafond de concurrence IOCP pour les threads qui retirent les paquets avec `GetQueuedCompletionStatus[Ex]`.
- [x] Libérer le slot actif d’un thread Win32 à sa terminaison, avec test dédié.
- [ ] Annuler les I/O déjà commencées et tester exhaustivement les buffers/handles invalides.
- [x] Ajouter `CreateProcessA/W`, les handles de processus, attente et codes de sortie pour des processus Linux/PE enfants (sous-ensemble).
- [x] Lire les blocs d’environnement ANSI ou Unicode de `CreateProcessA/W` et les appliquer au processus enfant.
- [x] Implémenter `CREATE_SUSPENDED`/`ResumeThread` pour les enfants via un lanceur Linux suspendu avant l’exécution du programme.
- [ ] Compléter le cycle de vie des processus : héritage de handles, attributs étendus et suivi/récolte robuste des enfants.
- [ ] Ajouter le partage d’objets nommés et de fichiers entre processus invités.
- [ ] Continuer `ntdll` : TLS statique des DLL chargées dynamiquement sur chaque thread, sections et primitives bas niveau manquantes (exceptions/unwinding SEH implémentés).
- [x] Étendre la CRT : chaînes étroites/larges, conversions (`strtol`, `strtod`, `_itoa`, `_itow`), classification de caractères, fonctions mathématiques, `qsort`/`bsearch` et formatage borné (`_snprintf`, `_snwprintf`), avec tests unitaires et PE (`crt_suite`).
- [x] Ajouter les flux `FILE*` de la CRT (`fopen`/`fclose`, `fread`/`fwrite`, `fgets`/`fputs`, `fseek`/`ftell`, `feof`, `fprintf`, `vfprintf`, `vsprintf`, `remove`/`rename`) avec tests unitaires et PE.
- [x] Ajouter les entrées formatées `scanf`/`sscanf`/`fscanf` et leurs variantes `v*` (entiers, flottants, chaînes, jeux de caractères, largeur, `*`, `%n`) avec tests unitaires et PE.
- [x] Sauvegarder/restaurer la position des flux CRT avec `fgetpos`/`fsetpos`, avec tests.
- [x] Continuer la CRT : locale et formats positionnels (`_snscanf` ajouté, `%N$` et `*N$` implémentés).
- [x] Ajouter `SetDIBitsToDevice`/`StretchDIBits` BI_RGB 1/4/8/16/24/32 bits avec tests de palette et de bornes.
- [ ] Poursuivre le rendu X11 : décorations/titre maison si absence de WM; compléter GDI (autres formats DIB/bitmap, ROP restants, polices/métriques complètes et texte sur DC mémoire).
- [ ] Intégrer un véritable runtime CLR Windows et les couches WPF/WinForms; le pont Linux `dotnet`/Mono ne suffit pas pour `paint.net`.
- [ ] Ajouter les DLL système nécessaires aux logiciels ciblés et mesurer leur compatibilité; la présence de stubs ne garantit pas l’API.
- [x] Décoder MP3 et AAC ADTS mono/stéréo dans PlaySound, tester limites mémoire, concurrence et sortie ALSA null ; inclure les bibliothèques dans l'ISO.
- [ ] Ajouter les autres formats audio compressés et les codecs vidéo ; Media Foundation/DirectShow ne constituent pas encore une chaîne de lecture complète.
- [x] Implémenter le vrai déchargement/référencement de DLL dans `FreeLibrary` (compteur de références, `DllMain(DLL_PROCESS_DETACH)` et `munmap`).
- [x] Exécuter de vrais programmes C compilés par MinGW-w64 : démarrage de la CRT msvcrt (`__getmainargs`, `_initterm`, constructeurs globaux, `atexit`/`_onexit`, `_iob`/`__iob_func`), `_errno`, verrous `_lock`, environnement, temps (`time`, `localtime`, `strftime`, `clock`), `rand` Microsoft, E/S bas niveau `_open`/`_read`/`_write`, signaux, `_beginthreadex`; compléments kernel32 (console, pages de code, `GetEnvironmentStrings`, `FormatMessage`, `lstr*`, `RaiseException`…). Vérifié par `testc/mingw_suite.c` (40 contrôles) en CI.
- [ ] Faire tourner des programmes C++ MinGW (déroulement SEH implémenté : `RtlCaptureContext`, `RtlVirtualUnwind`, `RtlUnwindEx` ; test `cpp_exceptions.cpp` ajouté) et des exécutables MSVC/clang-cl réels.
- [x] Documenter explicitement les limites : 64 bits uniquement, absence de noyau Windows et compatibilité API partielle.

## Priorité 3 — Double-clic et intégration Linux

- [x] Installer le runtime et le lanceur dans un chemin stable (`/usr/bin` par défaut, préfixe et `DESTDIR` configurables), avec association MIME par défaut dans `/etc/xdg/mimeapps.list`; validé dans une racine temporaire par `scripts/test-integration.sh`.
- [ ] Valider l’installation réelle sur l’hôte (`sudo integration/install.sh`) ; les tests en racine temporaire ne constituent pas cette preuve.
- [ ] Tester l’enregistrement binfmt sur une machine Linux réelle ; script de diagnostic et configuration présents, résultat réel non établi par cet audit.
- [x] Ajouter une procédure de désinstallation propre du binfmt, du MIME et du fichier `.desktop` (`integration/uninstall.sh`, restaure les associations précédentes).
- [x] Gérer les chemins contenant des espaces ou des URI `file://`, les fichiers illisibles et les volumes montés `noexec` (le lanceur lit le fichier au lieu de l’exécuter; le dossier du programme devient le répertoire courant).
- [x] Définir le comportement du terminal : `winlux --inspect` lit le sous-système PE, `winlux-open` ouvre un terminal (Konsole, GNOME Terminal, XFCE, xterm ou `WINLUX_TERMINAL`) qui reste ouvert pour les programmes console et lance directement les programmes graphiques.
- [x] Ajouter une détection du fichier `.exe` et des messages d’erreur graphiques (kdialog, zenity, notify-send, xmessage) ou textuels adaptés.
- [ ] Tester le double-clic depuis GNOME, KDE et XFCE ; la présence de `test-binfmt-desktops.sh` ne prouve pas ces trois sessions.

## Priorité 4 — ISO de test

- [x] Construire une base Linux minimale en initramfs pour l’ISO de validation.
- [x] Construire automatiquement l’arborescence de staging.
- [x] Installer Winlux, les dix PE, leur DLL et Xvfb dans cette arborescence.
- [x] Intégrer binfmt, MIME et `.desktop` dans une future image de bureau; ils ne sont pas nécessaires à l’image de validation.
- [x] Ajouter un démarrage BIOS et UEFI avec GRUB.
- [x] Lancer automatiquement les tests séquentiels, parallèles et X11 dans la VM.
- [x] Générer l’ISO avec `xorriso` et produire une somme SHA-256.
- [x] Ajouter un test automatisé dans QEMU avec vérification des marqueurs de résultat.
- [x] Sélectionner l'entrée de diagnostic dans `scripts/test-live-iso-qemu.sh` et vérifier BIOS/UEFI sans modifier l'ISO livrée.
- [x] Documenter la RAM, le nombre de CPU et les options nécessaires à la VM.

## Priorité 5 — Thème Windows 11

- [x] VM : pointeur flèche visible (curseur logiciel Xorg `SWcursor`, flèche dessinée à l’échelle de l’interface).
- [x] VM : la résolution suit la taille de la fenêtre QEMU (nouveau mode préféré appliqué par `xrandr`, bureau relu via `GetSystemMetrics` et réagencé); vérifié en passant de 1920×1080 à 1280×800 dans QEMU.
- [x] Refaire l’interface du bureau façon Windows 11 : fond « fleur », barre des tâches sombre centrée ou à gauche (`WINLUX_TASKBAR_ALIGN=left` ou bouton du menu), menu Démarrer flottant (recherche, épinglés, recommandés, utilisateur, alimentation), zone de notification et icônes vectorielles.
- [x] Mémoriser l’alignement de la barre des tâches et ouvrir de vraies applications depuis la barre et le menu.

- [x] Ajouter un prototype de bureau GDI HiDPI : fond d'écran, raccourcis,
  barre des tâches pleine largeur, menu Démarrer inspiré de Cinnamon,
  applications épinglées et zone système réseau/volume/batterie/heure à droite.
- [x] Reprendre le prototype avec un système visuel professionnel : surfaces
  arrondies, profondeur, palette cohérente et hiérarchie typographique.
- [x] Charger les vraies icônes PNG des applications depuis leurs fichiers
  `.desktop` et thèmes XDG, avec cache, alpha et interpolation bilinéaire HiDPI.
- [x] Utiliser Rust/GDI avec backend X11 et Xft pour le prototype actuel ; le choix d'une pile de bureau définitive reste ouvert.
- [x] Créer les composants du prototype : barre des tâches, menu Démarrer, fenêtres, notifications et Explorer ; le CSS reste une référence séparée.
- [x] Ajouter les icônes et polices libres compatibles avec la licence du projet.
- [x] Définir les variantes clair/sombre et le réglage d'échelle.
- [ ] Compléter l'accessibilité : taille du pointeur et navigation accessible complète (contraste élevé et réduction des animations ajoutés).
- [x] Intégrer le thème dans l’ISO sans dépendance propriétaire.
- [ ] Revalider le rendu du bureau courant en 1080p, 1440p et 4K, avec captures et interactions ; les tests X11 généraux ne prouvent pas cette matrice complète.
- [x] Rendre le menu Démarrer plus compact, animer les accents violets et afficher
  des indicateurs réseau, volume et batterie sans libellés dans la zone système.
- [x] Permettre le déplacement des icônes du prototype de bureau (placement
  recalé sur une grille, positions temporaires).
- [x] Démarrer avec un bureau vide ; rendre les icônes système facultatives et
  désactivées par défaut dans Paramètres.
- [ ] Ajouter la création explicite de raccourcis d'applications et le
  choix entre placement sur grille ou libre (positions des icônes du bureau persistées).

## Priorité 6 — Qualité et livraison

- [x] Ajouter une CI pour `cargo check`, les builds des `.exe` de test et les tests sous Winlux.
- [x] Normaliser le code existant avec `rustfmt`, traiter la dette Clippy puis rendre ces contrôles bloquants dans la CI.
- [x] Ajouter des logs structurés et un niveau de verbosité configurable.
- [x] Documenter la compilation, l’installation, le dépannage et la génération ISO.
- [x] Ajouter un paquet de mise à jour GitHub vérifié par SHA-256, une installation
  transactionnelle avec récupération et un retour arrière automatisé ; pas de garantie A/B.
- [x] Créer une VM graphique persistante QCOW2 issue de la racine Winlux.
- [x] Ajouter une licence GPL-3.0-or-later.
- [x] Fournir BUILD-INFO et sa consultation dans Paramètres → À propos.
- [x] Sécuriser l'installateur et valider le disque installé sans ISO en BIOS/UEFI sous QEMU, avec arrêt automatique.
- [x] Ajouter les barrières de tests Live/disque installé au workflow de release ; son exécution complète sur les runners GitHub reste à valider.
- [x] Ajouter le fichier NOTICES au dépôt et copier les notices audio disponibles dans les ISO.
- [ ] Vérifier et embarquer toutes les notices et les obligations de fourniture de sources pour une distribution complète ; NOTICES seul n'est pas une preuve d'empaquetage exhaustif.
- [x] Publier ISO, sommes, paquet, manifeste et notes dans les préversions système existantes (notamment preview.4).
- [ ] Valider une exécution complète du workflow de release modifié sur GitHub ; ses étapes de construction et tests ne constituent pas une publication déjà effectuée.
- [ ] Signer les mises à jour (Ed25519) et implémenter le schéma A/B réel (voir `docs/AB_UPDATES.md`).
- [ ] Committer une première version stable après validation du pipeline complet.
