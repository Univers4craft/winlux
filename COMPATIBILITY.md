# Compatibilité Linux / Windows

Winlux vise une interopérabilité maximale, mais une compatibilité garantie à
100 % et zéro crash pour *tous* les logiciels Windows et Linux n’est pas
réalisable avec quelques DLL de remplacement. Les programmes dépendent aussi
du noyau Windows, de son ordonnanceur, de DirectX, du registre, des pilotes,
des codecs, des services et parfois d’un runtime managé.

## Ce qui est déjà partagé

- Les exécutables PE64 peuvent être chargés directement par le runtime.
- `C:\` est mappé vers le répertoire utilisateur Linux.
- `Z:\` expose la racine Linux.
- Les chemins Windows sont convertis avec recherche insensible à la casse.
- Les variables d’environnement, la ligne de commande et les sorties console
  sont reliées au processus Linux.
- Plusieurs API mémoire, TLS, temps, Unicode, fichiers, GUI de base et réseau
  sont bridées.
- Des DLL courantes sont intégrées : `user32`, `gdi32`, `advapi32`, `ole32`,
  `bcrypt`, `version`, `shell32`, `ws2_32`, `mfplat` et `winmm`.
- Le registre de base est partagé avec Linux et persiste dans
  `$XDG_DATA_HOME/winlux/registry.tsv` (ou `~/.local/share/winlux`).
- Le SCM local persiste les définitions de services dans le registre Winlux et
  peut lancer/arrêter leurs exécutables en processus utilisateur, sans privilèges.
  `StartServiceCtrlDispatcher` et les notifications complètes du SCM restent
  non implémentés.
- Les événements, mutex et attentes de base sont mappés sur des primitives
  thread-safe Linux.
- Le runtime gère le chargement dynamique de DLL réelles avec `LoadLibraryA/W`,
  `GetProcAddress` par nom ou ordinal, les callbacks TLS et `DllMain` au chargement.
- `CreateProcessA/W` peut lancer des exécutables Linux natifs ou relancer un PE via
  Winlux; les handles offrent attente et codes de sortie, et les blocs
  d’environnement ANSI/Unicode sont appliqués. `CREATE_SUSPENDED`/`ResumeThread`
  utilisent un petit lanceur Linux qui s’arrête avant `exec`. L’héritage des handles
  et les attributs de création avancés ne sont pas pris en charge.
- Les variantes Unicode `W`, les conversions UTF-8/Windows-1252, `VirtualProtect`,
  `MEM_DECOMMIT`, `GetSystemInfo` et plusieurs fonctions `Rtl*` sont disponibles.
- Les API de fichiers A/W incluent notamment `CreateFile`, `CopyFile`, `MoveFile`,
  `DeleteFile`, `CreateDirectory`, `RemoveDirectory`, `GetFullPathName` et
  l’énumération `FindFirstFile`/`FindNextFile` avec jokers, `GetTempFileName`,
  `GetFileAttributesEx` et `SetEndOfFile`.
- `ReadFile`/`WriteFile` overlapped utilisent des offsets explicites, retournent
  `ERROR_IO_PENDING` et se récupèrent avec `GetOverlappedResult`; l’annulation,
  avec `CancelIo/CancelIoEx`, est best-effort avant le début effectif de l’accès.
  Un sous-ensemble de ports IOCP est disponible (`CreateIoCompletionPort`,
  `GetQueuedCompletionStatus[Ex]`, `PostQueuedCompletionStatus`) et testé avec des
  I/O overlapped associées et la lecture par lot. Le plafond de concurrence est
  appliqué aux workers; un worker libère son slot lorsqu’il rappelle GQCS. La
  sortie d’un worker sans nouvel appel et l’annulation d’E/S déjà bloquante
  restent incomplètes.
- Winsock relie les sockets POSIX, les codes d’erreur WSA et les résolutions
  `getaddrinfo` A/W; TCP loopback, `bind/listen/connect/accept/send/recv` sont
  testés, mais ce n’est pas encore une implémentation complète de Winsock.
- `VirtualQuery`, les sections critiques récursives et `WaitForMultipleObjects`
  wait-any/wait-all sont pris en charge pour les événements, mutex et threads
  suivis par Winlux. `VirtualQuery` décrit les régions mémoire que Winlux a
  lui-même réservées/commitées, pas l’ensemble des mappings Linux du processus.
- Les opérations fichier A/W couvrent copie, déplacement, chemins absolus et
  répertoires; partage Windows avancé, handles hérités et I/O overlapped restent
  absents.
- La CRT (`msvcrt`/`ucrtbase`) fournit les chaînes étroites et larges courantes,
  les conversions (`atoi`/`strtol`/`strtod`/`_itoa`/`_itow`), la classification de
  caractères, les fonctions mathématiques usuelles, `qsort`/`bsearch`, le
  formatage borné étroit et large (`_snprintf`/`_snprintf_s`/`_snwprintf`) et une
  couche de flux `FILE*` (`fopen`/`fread`/`fwrite`/`fgets`/`fseek`/`fprintf`/
  `remove`/`rename`) et les entrées formatées `scanf`/`sscanf`/`fscanf`.
  La locale, `_snscanf` et `fgetpos`/`fsetpos` restent absents.
- Le registre propose un sous-ensemble Win32 et une persistance propriétaire
  TSV; il ne reproduit ni les vues 32/64 bits ni les règles ACL du registre Windows.
- Les DLL intégrées utilisent des fonctions Linux natives et ne dépendent pas
  de Wine ni de bibliothèques Windows existantes.

## Stratégie d’intercouche

1. **Couche système** : handles, fichiers, répertoires, threads, synchronisation,
   mémoire virtuelle, processus et signaux.
2. **Couche Win32** : `kernel32`, `ntdll`, `advapi32`, `user32`, `ole32`,
   `shell32`, `version`, `bcrypt` et autres DLL fréquentes.
3. **Couche graphique** : fenêtres, messages, GDI, Direct2D/DirectWrite et
   traduction vers une pile Linux choisie.
4. **Couche runtime** : CRT, .NET/CLR, Visual C++ runtime, codecs et composants
   nécessaires au logiciel ciblé.
5. **Validation** : tests par logiciel, journal des imports manquants, tests de
    non-régression et exécution dans QEMU.

Les codecs ne sont pas une DLL universelle : les points d’entrée Media Foundation
ne constituent pas encore une chaîne de décodage complète et sont encore partiels.

La passerelle `winmm!PlaySoundA/W` décode déjà les fichiers RIFF/WAVE PCM 8/16/24/32 bits
et IEEE float 32 bits, puis tente une sortie ALSA dynamique via `libasound.so.2`.
Le parseur est testé; la lecture physique dépend de la présence d’ALSA et d’un périphérique
audio accessible, et n’a pas été validée sur plusieurs machines. Les codecs compressés,
Media Foundation/DirectShow et la lecture vidéo restent à faire.

`FreeLibrary` ne démonte pas encore le mapping PE : les DLL chargées restent
présentes jusqu’à la fin du processus afin de ne pas invalider des pointeurs
encore détenus par le code invité. Les handles de fichiers ignorent encore les
partages Windows avancés et les opérations asynchrones/overlapped.

Le noyau fenêtres/messages (`user32`) fonctionne : classes, table HWND, file de
messages par thread, appel du `WndProc` invité et une boucle `GetMessage`/
`DispatchMessage` complète (testée par `gui_suite.exe` et `gui_demo.exe`).
Deux backends sont disponibles : *headless* (par défaut, aucun affichage, sert
aux tests) et **X11** (`gui_x11.rs`), qui crée de vraies fenêtres sur le serveur
X, rend le thème « blob » (dégradé vertical violet→bleu avec accent) et traduit
les événements X en messages Win32 (`Expose`→`WM_PAINT`, `ConfigureNotify`→
`WM_SIZE`, boutons et mouvements de souris, keysyms→`VK_*`/`WM_KEYDOWN`/
`WM_KEYUP`, `WM_DELETE_WINDOW`→`WM_CLOSE`). `TranslateMessage` publie les
`WM_CHAR` correspondants pour les caractères ASCII/Latin-1 et les caractères
BMP Unicode pris en charge ; la composition IME, les caractères hors BMP et
les API d’état/focus clavier restent à implémenter.

La couche de dessin GDI est amorcée : les primitives invitées n’écrivent pas
directement dans la fenêtre, elles empilent des commandes (`FillRect`,
`FrameRect`, `Ellipse`, `Text`) associées à la fenêtre du `HDC`. `EndPaint` et
`ReleaseDC` transmettent la liste au backend, qui la rejoue après le thème à
chaque `Expose` : le contenu survit donc aux rafraîchissements et aux
redimensionnements. Sont couverts `CreateSolidBrush`, `CreatePen`,
`SelectObject`, `DeleteObject`, `GetStockObject` (pinceaux/plumes standard),
`SetTextColor`, `SetBkColor`, `SetBkMode` (texte opaque ou transparent),
`TextOutA`, `Rectangle`, `Ellipse`, `FillRect`, `FrameRect`, `GetDC` et
`ReleaseDC`. Le texte est rendu par la police « core » du serveur X. Manquent
encore les polices GDI, les bitmaps/`BitBlt`, les régions, les couleurs système
et le double tampon. L’association `.desktop` et binfmt est préparée, mais n’a pas été
installée ni validée sur une session de bureau réelle. Le script ISO requiert
encore un staging Linux bootable.

## Cas .NET et paint.net

`paint.net` est un logiciel .NET et graphique. Pour l’exécuter réellement, il
faudra intégrer un CLR compatible et implémenter une pile GUI suffisante. Le
module `mscoree` actuel fournit un diagnostic explicite, mais ne prétend pas
exécuter du code managé.

## Politique anti-crash

Winlux transforme déjà plusieurs fautes Linux en diagnostics Windows et
journalise les imports inconnus. Cela ne peut pas garantir l’absence de crash :
une API absente, un pilote Windows, une hypothèse mémoire incorrecte ou une
incompatibilité graphique peuvent provoquer un échec fonctionnel. La bonne
approche est donc de détecter, diagnostiquer et isoler chaque incompatibilité,
plutôt que de masquer toutes les erreurs avec des valeurs arbitraires.
