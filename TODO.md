# Feuille de route Winlux

## État actuel

- Dernière validation connue : `cargo test -p winlux` (47 tests), compilation release et les neuf exécutables PE de test réussissent. Relancer les scripts après chaque changement.
- [x] Charger un exécutable PE32+ x86-64.
- [x] Appliquer les relocations et résoudre les imports de base.
- [x] Initialiser un PEB/TEB minimal via `GS`.
- [x] Fournir les API nécessaires aux tests P1.
- [x] Implémenter les ponts `printf`/`sprintf` variadiques Windows x64.
- [x] Exécuter `hello.exe`, `exitcode.exe`, `p1_suite.exe`, `dll_import.exe`, `delay_import.exe`, `findfiles.exe`, `crt_suite.exe`, `gui_suite.exe` et `gui_demo.exe` sous Winlux.
- [x] Ajouter 47 tests unitaires couvrant le runtime, le registre, le SCM, WAVE, Winsock, les processus, les fichiers, la CRT, IOCP et le noyau fenêtres/messages.
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
- [ ] Implémenter une pile de secours réellement sûre pour les débordements de pile; le gestionnaire de signal est sans allocation/verrou, mais la protection contre le débordement n’est pas validée.
- [x] Éviter les allocations et verrous dans le gestionnaire de signaux de crash.
- [x] Définir la politique pour les imports manquants : journalisation, arrêt par défaut et mode explicite `WINLUX_MISSING=ignore`.
- [x] Renforcer les vérifications de bornes des sections, imports et IAT avant accès mémoire.

## Priorité 2 — Compatibilité Windows

- [x] Ajouter des DLL intégrées courantes : `user32`, `advapi32`, `ole32`, `bcrypt`, `version` et `shell32`.
- [x] Ajouter un diagnostic explicite pour les exécutables gérés dépendant de `mscoree.dll`.
- [x] Relier les API de base fichiers/répertoires Windows au système de fichiers Linux.
- [x] Ajouter des variantes Unicode `W` pour les chemins, fichiers et variables d’environnement.
- [x] Ajouter `GetFullPathName`, `CopyFile`, `MoveFile`, `CreateDirectory` et `RemoveDirectory` en A/W ou A selon l’API.
- [x] Ajouter `GetTempFileNameA/W`, `GetFileAttributesExA/W` et `SetEndOfFile` avec tests fonctionnels.
- [x] Ajouter l’énumération de répertoires `FindFirstFileA/W`, `FindNextFileA/W` et `FindClose` avec motifs `*`/`?` et tests PE.
- [x] Implémenter les conversions UTF-8 et Windows-1252 avec vérification des buffers.
- [x] Ajouter les API de base `GetSystemInfo`, version OS, horloge FILETIME et plusieurs primitives `Rtl*`.
- [x] Ajouter les premières API GUI (`user32`, `gdi32`) et réseau (`ws2_32`).
- [x] Construire le noyau fenêtres/messages (`gui.rs`) : classes, table HWND, file par thread, appel du `WndProc` invité, thème « blob » violet-bleu, avec un test PE `gui_suite.exe` (séquence WM_CREATE/WM_SIZE/WM_PAINT/WM_DESTROY).
- [x] Relier le noyau à un backend X11 réel (`gui_x11.rs`, x11rb) : vraies fenêtres, dégradé violet→bleu + accent rendus côté serveur, événements X traduits en messages Win32 (`Expose`→`WM_PAINT`, `ConfigureNotify`→`WM_SIZE`, souris, clavier X11→`VK_*`/`WM_KEYDOWN`/`WM_KEYUP`, `WM_DELETE_WINDOW`→`WM_CLOSE`). `TranslateMessage` produit `WM_CHAR` (ASCII/Latin-1 et BMP) ; tracé GDI et saisie vérifiés par `gui_demo.exe` sous Xvfb. Backend *headless* conservé pour les tests (`WINLUX_GUI=headless`).
- [x] Implémenter `SetWindowPos` et `MoveWindow` : subset de flags `SWP_NOMOVE`/`SWP_NOSIZE`/`SWP_NOREDRAW`, rectangles HWND actualisés, messages `WM_MOVE`/`WM_SIZE`, configuration de la vraie fenêtre X11 et synchronisation des positions/tailles via `ConfigureNotify`; testé par `gui_suite.exe` en headless et sous Xvfb.
- [x] Ajouter le focus clavier par thread (`GetFocus`/`SetFocus`), les notifications synchrones `WM_SETFOCUS`/`WM_KILLFOCUS`, la perte de focus à la destruction et la traduction des événements X11 `FocusIn`/`FocusOut`; testé par `gui_suite.exe` et par injection de focus sous Xvfb.
- [x] Ajouter `GetKeyState`, `GetAsyncKeyState` et `GetKeyboardState` : état thread mis à jour lors du retrait de `WM_KEYDOWN`/`WM_KEYUP`, état asynchrone alimenté par les événements X11, bits pressé/bascule et tableau de 256 touches; tests unitaires, PE et saisie Xvfb.
- [x] Ajouter la couche de dessin GDI : commandes (`FillRect`, `FrameRect`, `Ellipse`, `Text`) accumulées par cycle de peinture puis rejouées par le backend (y compris à chaque `Expose`), pinceaux/plumes/polices (`CreateSolidBrush`, `CreatePen`, `CreateFontA/W`, `SelectObject`, `DeleteObject`, `GetStockObject`), `GetSysColor`/`GetSysColorBrush`, `PatBlt` (PATCOPY/BLACKNESS/WHITENESS), couleurs et mode de fond (`SetTextColor`, `SetBkColor`, `SetBkMode`), `TextOutA`, `Rectangle`, `Ellipse` et `GetDC`/`ReleaseDC`. `gui_demo.exe` vérifie le tracé et le rendu a été contrôlé par capture X11 (`xwd`).
- [x] Relier sockets TCP/UDP, erreurs WSA et résolveur `getaddrinfo` A/W aux interfaces POSIX (tests loopback).
- [x] Ajouter un registre persistant partagé Linux/Windows.
- [x] Ajouter une couche SCM locale, non privilégiée, qui persiste les définitions et lance/arrête les services en processus utilisateur.
- [ ] Implémenter `StartServiceCtrlDispatcher` et le canal de contrôle/notification des vrais services Windows.
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
- [ ] Continuer `ntdll` : exceptions/unwinding, TLS statique des DLL chargées dynamiquement sur chaque thread, sections et primitives bas niveau manquantes.
- [x] Étendre la CRT : chaînes étroites/larges, conversions (`strtol`, `strtod`, `_itoa`, `_itow`), classification de caractères, fonctions mathématiques, `qsort`/`bsearch` et formatage borné (`_snprintf`, `_snwprintf`), avec tests unitaires et PE (`crt_suite`).
- [x] Ajouter les flux `FILE*` de la CRT (`fopen`/`fclose`, `fread`/`fwrite`, `fgets`/`fputs`, `fseek`/`ftell`, `feof`, `fprintf`, `vfprintf`, `vsprintf`, `remove`/`rename`) avec tests unitaires et PE.
- [x] Ajouter les entrées formatées `scanf`/`sscanf`/`fscanf` et leurs variantes `v*` (entiers, flottants, chaînes, jeux de caractères, largeur, `*`, `%n`) avec tests unitaires et PE.
- [ ] Continuer la CRT : locale, `_snscanf`, `fgetpos`/`fsetpos` et formats positionnels.
- [ ] Poursuivre le rendu X11 : décorations/titre maison si absence de WM; compléter GDI (bitmaps/`BitBlt`/`StretchBlt`, autres ROP de `PatBlt`, polices et métriques complètes).
- [ ] Intégrer un véritable runtime CLR compatible pour exécuter les `.exe` .NET; sans CLR, `paint.net` ne fonctionne pas.
- [ ] Ajouter les DLL système nécessaires aux logiciels ciblés et mesurer leur compatibilité; la présence de stubs ne garantit pas l’API.
- [ ] Ajouter les formats audio compressés et les codecs vidéo; Media Foundation n’est pas encore reliée à une chaîne complète.
- [ ] Implémenter le vrai déchargement/référencement de DLL dans `FreeLibrary` (elles restent mappées jusqu’à la fin du processus).
- [ ] Tester des exécutables compilés avec MSVC, MinGW et clang-cl, pas uniquement les tests Rust.
- [x] Documenter explicitement les limites : 64 bits uniquement, absence de noyau Windows et compatibilité API partielle.

## Priorité 3 — Double-clic et intégration Linux

- [ ] Installer le binaire dans un chemin stable (`/usr/bin/winlux` ou `/usr/local/bin/winlux`); l’installation n’a pas été validée sur l’hôte.
- [ ] Tester l’enregistrement binfmt sur une machine Linux réelle (fichier de configuration préparé, mais non installé/validé).
- [ ] Ajouter une procédure de désinstallation propre du binfmt, du MIME et du fichier `.desktop`.
- [ ] Gérer les chemins contenant des espaces, les permissions et les exécutables sur volumes montés.
- [ ] Définir le comportement du terminal pour les programmes console et GUI (le `.desktop` actuel force encore `Terminal=true`).
- [ ] Ajouter une détection du fichier `.exe` et des messages d’erreur graphiques ou textuels adaptés.
- [ ] Tester le double-clic depuis plusieurs environnements de bureau : GNOME, KDE et XFCE.

## Priorité 4 — ISO de test

- [ ] Choisir une base Linux minimale pour l’ISO; le script actuel n’engendre pas à lui seul une image bootable.
- [ ] Construire automatiquement l’arborescence de staging.
- [ ] Installer Winlux, binfmt, MIME, `.desktop` et les tests dans cette arborescence.
- [ ] Ajouter un démarrage BIOS et UEFI avec GRUB ou un autre chargeur adapté.
- [ ] Ajouter un script de lancement automatique des tests P1 dans la VM; `scripts/test-runtime.sh` teste actuellement les binaires localement.
- [ ] Générer l’ISO avec `xorriso` et produire une somme SHA-256.
- [ ] Ajouter un test automatisé dans QEMU avec sortie et code de retour vérifiés.
- [ ] Documenter la RAM, le nombre de CPU et les options nécessaires à la VM.

## Priorité 5 — Thème Windows 11

- [ ] Choisir la pile graphique de l’environnement cible : GTK, Qt ou autre.
- [ ] Créer les composants : barre des tâches, menu de démarrage, fenêtres, notifications et explorateur de fichiers; le CSS actuel est une palette isolée, pas un shell graphique.
- [ ] Ajouter les icônes et polices libres compatibles avec la licence du projet.
- [ ] Définir les variantes clair/sombre et les paramètres d’accessibilité.
- [ ] Intégrer le thème dans l’ISO sans dépendance propriétaire.
- [ ] Tester le rendu sur différentes résolutions et facteurs d’échelle.

## Priorité 6 — Qualité et livraison

- [ ] Ajouter une CI pour `cargo check`, les builds des `.exe` de test et les tests sous Winlux.
- [ ] Ajouter `rustfmt` et `clippy` avec une configuration stable.
- [ ] Ajouter des logs structurés et un niveau de verbosité configurable.
- [ ] Documenter la compilation, l’installation, le dépannage et la génération ISO.
- [ ] Ajouter une licence et les mentions de provenance des composants externes.
- [ ] Créer des releases reproductibles avec binaires, ISO, sommes et notes de version.
- [ ] Committer une première version stable après validation du pipeline complet.
