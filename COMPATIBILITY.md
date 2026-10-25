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
- La fondation COM/OLE gère l’initialisation d’appartement par thread,
  `CoTaskMemAlloc/Realloc/Free`, les GUID, la mémoire globale Win32 et un vrai
  `IStream` en mémoire partageant son `HGLOBAL` (lecture, écriture, seek, taille,
  métadonnées, copie et clone). `CoRegisterClassObject`/`CoRevokeClassObject`
  et `CoCreateInstance` prennent en charge les fabriques multi-usages enregistrées
  dans le processus, sans agrégation. L’activation depuis le registre, les serveurs
  hors processus et l’automatisation restent non implémentés.
- Le registre de base est partagé avec Linux et persiste dans
  `$XDG_DATA_HOME/winlux/registry.tsv` (ou `~/.local/share/winlux`).
- Le SCM local persiste les définitions de services dans le registre Winlux et
  peut lancer/arrêter leurs exécutables en processus utilisateur, sans privilèges.
  `StartServiceCtrlDispatcher` et les notifications complètes du SCM restent
  non implémentés.
- Les événements, mutex et attentes de base sont mappés sur des primitives
  thread-safe Linux. `OpenEventA`/`OpenMutexA` rouvrent les objets nommés du processus ;
  l'héritage des handles n'est pas pris en charge.
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
  `fgetpos`/`fsetpos` permettent de sauvegarder/restaurer la position des flux.
  La locale et `_snscanf` restent absents.
- Le registre propose un sous-ensemble Win32 et une persistance propriétaire
  TSV; il ne reproduit ni les vues 32/64 bits ni les règles ACL du registre Windows.
- Les DLL intégrées utilisent des fonctions Linux natives et ne dépendent pas
  de Wine ni de bibliothèques Windows existantes.
- Les PE managés 32/64 bits sont détectés avant le chargeur natif. Un apphost
  moderne accompagné de son `.dll` et de son `.runtimeconfig.json` est relayé
  vers `dotnet`; un ancien assembly CLR est relayé vers Mono. Le runtime doit
  être installé sur l'hôte (`WINLUX_DOTNET`/`WINLUX_MONO` permettent de choisir
  son chemin).

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
L'alias `mfreadwrite` expose des points d'entrée renvoyant une erreur explicite pour
les lecteurs/rédacteurs non implémentés ; `quartz` fournit uniquement les messages
d'erreur DirectShow. Aucun faux objet COM de lecture vidéo n'est exposé.

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
X, rend une palette claire inspirée de Windows 11 avec accent bleu et traduit
les événements X en messages Win32 (`Expose`→`WM_PAINT`, `ConfigureNotify`→
`WM_SIZE`, boutons et mouvements de souris, keysyms→`VK_*`/`WM_KEYDOWN`/
`WM_KEYUP`, `WM_DELETE_WINDOW`→`WM_CLOSE`). `TranslateMessage` publie les
`WM_CHAR` correspondants pour les caractères ASCII/Latin-1 et les caractères
BMP Unicode pris en charge ; la composition IME, les caractères hors BMP et
les touches mortes restent à implémenter. `GetKeyState`, `GetAsyncKeyState` et
`GetKeyboardState` sont pris en charge en sous-ensemble : état de thread mis à
jour au retrait des messages clavier, état asynchrone alimenté par X11 et
tableau de 256 touches virtuelles. `GetFocus`/`SetFocus` fonctionnent par thread
et `FocusIn`/`FocusOut` déclenchent `WM_SETFOCUS`/`WM_KILLFOCUS`. `SetWindowPos` et
`MoveWindow` déplacent/redimensionnent les fenêtres, actualisent les rectangles
Win32 et publient `WM_MOVE`/`WM_SIZE` ; les changements de taille externes via
`ConfigureNotify` sont également reflétés, de même que les déplacements externes.
Les variantes A/W des principales boucles de messages et les titres de fenêtre
Unicode (`Set/GetWindowTextW`) partagent le même noyau et sont testés par un PE.
Le z-order et les notifications
`WINDOWPOS` avancées ne sont pas encore gérés.

Le prototype de shell utilise ce backend pour rendre un bureau HiDPI avec
papier peint vectoriel animé, barre des tâches, menu Démarrer et zone système
réseau/audio/batterie. Le code affiche actuellement trois icônes système
prédéfinies (« Ce PC », « Documents », « Corbeille »), les déplace en mémoire
et les recale sur une grille ; leur visibilité et leur position ne sont pas
encore des préférences persistantes. Ce comportement devra évoluer vers un
bureau vide par défaut, avec raccourcis créés volontairement et icônes système
activables dans Paramètres. Les icônes PNG des applications Linux peuvent être
lues depuis les fichiers `.desktop` et thèmes XDG ; elles sont décodées avec
alpha, mises en cache et filtrées lors des changements d’échelle. Les couleurs
X11 allouées sont également mises en cache afin que les animations longues ne
saturent pas la palette du serveur. Il s’agit encore d’un prototype dans
`gui_demo.exe`, pas d’un gestionnaire de bureau autonome.

Le mécanisme `winlux-update` récupère le manifeste de la dernière release
GitHub, avec jeton de lecture configuré dans Paramètres pour un dépôt privé.
Les paquets système incluent bureau, outils, applications embarquées, noyau,
modules et bibliothèques ; les paquets runtime réduits sont explicitement
identifiés. Inventaires, SHA-256, chemins et liens sont vérifiés, les fichiers
précédents sauvegardés et une installation interrompue restaurée. Les données
et configurations sont conservées. Une relance de session ou un redémarrage
explicite est demandé selon les composants changés. La protection A/B contre
une panne d'alimentation reste à ajouter.

La couche de dessin GDI est amorcée : les primitives invitées n’écrivent pas
directement dans la fenêtre, elles empilent des commandes (`FillRect`,
`FrameRect`, `Ellipse`, `Text`) associées à la fenêtre du `HDC`. `EndPaint` et
`ReleaseDC` transmettent la liste au backend, qui la rejoue après le thème à
chaque `Expose` : le contenu survit donc aux rafraîchissements et aux
redimensionnements. Sont couverts `CreateSolidBrush`, `CreatePen`,
`SelectObject`, `DeleteObject`, `GetStockObject` (pinceaux/plumes standard),
`GetSysColor`/`GetSysColorBrush` pour le sous-ensemble de couleurs du thème,
`GetObjectA/W` pour les bitmaps, pinceaux, plumes et polices pris en charge,
`SetTextColor`, `SetBkColor`, `SetBkMode` (texte opaque ou transparent),
`TextOutA`, `Rectangle`, `Ellipse`, `FillRect`, `FrameRect`, `PatBlt` (PATCOPY,
BLACKNESS, WHITENESS, PATINVERT, PATAND, PATOR, DSTINVERT), `BitBlt`/`StretchBlt`
avec un sous-ensemble de ROP (SRCCOPY, SRCPAINT, SRCAND, SRCINVERT, NOTSRCCOPY,
DSTINVERT, BLACKNESS, WHITENESS), `CreateCompatibleBitmap`, `CreateDIBSection`
32bpp BI_RGB et blits entre bitmap mémoire et fenêtre, `GetDC` et `ReleaseDC`.
`CreateFontA/W` utilise Xft/Fontconfig pour un texte vectoriel antialiasé et le
backend applique une échelle DPI commune aux fenêtres, formes, textes et entrées.
Les polices X11 core ne servent plus que de repli; plusieurs attributs LOGFONT
restent approximatifs. `SetDIBitsToDevice` et `StretchDIBits` traitent également
les DIB BI_RGB 1/4/8/16/24/32 bits, avec palettes RGB, limites de dimensions et
redimensionnement au plus proche voisin. Les bandes partielles et DIB_PAL_COLORS
restent non pris en charge. Manquent encore les autres compressions DIB, les autres
ROP, les régions
et le double tampon.
L’association `.desktop` et binfmt est préparée, mais n’a pas été
installée ni validée sur une session de bureau réelle. Le script ISO produit
maintenant le staging Linux bootable utilisé par les tests et la VM persistante.

## Objectif de compatibilité .NET Windows

La compatibilité des applications Windows .NET est un objectif du projet, et
pas seulement une limite à documenter. La cible à terme comprend les
applications .NET Framework et .NET modernes qui dépendent d'API Windows, ainsi
que les interfaces WinForms et WPF. Le pont Linux actuel ne remplit pas cet
objectif à lui seul :

- Winlux détecte les assemblies CLR PE32/PE32+ et les apphosts .NET modernes.
- Les programmes managés réellement multiplateformes peuvent être relayés vers
  `dotnet` Linux ou Mono lorsqu'un runtime compatible est installé.
- Ce relais n'exécute pas un CLR Windows et ne fournit pas les API Win32, les
  composants natifs ou les dépendances Windows attendues par une application
  .NET de bureau.
- WinForms et WPF nécessitent leurs propres couches de compatibilité graphique
  et système. Un assembly qui démarre sous `dotnet` Linux n'est donc pas une
  preuve de compatibilité WinForms/WPF.

Le travail est à mener par étapes et à valider avec des applications de
référence :

1. Définir les versions de .NET, les architectures et les applications de
   bureau prioritaires ; détecter les dépendances manquantes avant le lancement
   et produire des diagnostics exploitables.
2. Construire une voie d'exécution fiable pour les assemblies et apphosts
   Windows, avec résolution des runtimes et dépendances, comportement de
   lancement cohérent, codes de sortie et journalisation. Le relais vers le
   runtime Linux reste un chemin distinct pour les applications réellement
   multiplateformes.
3. Étendre les API système, le chargement des DLL natives, COM/OLE, la CRT et
   les API graphiques requises par les applications choisies ; ne pas traiter
   une simple résolution d'import comme une implémentation fonctionnelle.
4. Ajouter WinForms par incréments, en couvrant le cycle de vie des fenêtres,
   les contrôles, les événements, le dessin, le texte, le DPI et les dialogues
   employés par les applications de référence.
5. Étudier puis construire les composants requis par WPF : runtime managé,
   liaison de données et ressources, rendu texte, composition et primitives
   graphiques. WPF constitue une étape distincte et plus vaste que le seul
   lancement d'un assembly CLR.
6. Maintenir des tests d'intégration reproductibles par version et application,
   et publier pour chacune les fonctions validées, les régressions et les
   limites connues. `paint.net` peut servir de cible de référence, sans être
   considéré comme compatible avant que ses scénarios réels passent ces tests.

La stratégie d'implémentation du CLR et des couches de bureau reste à choisir
sur la base des applications prioritaires et de prototypes mesurables. Le
module `mscoree` conserve un diagnostic explicite lorsqu'aucun chemin
d'exécution pris en charge ne s'applique ; il ne doit pas masquer une
incompatibilité par un succès artificiel.

## Politique anti-crash

Winlux transforme déjà plusieurs fautes Linux en diagnostics Windows et
journalise les imports inconnus. Cela ne peut pas garantir l’absence de crash :
une API absente, un pilote Windows, une hypothèse mémoire incorrecte ou une
incompatibilité graphique peuvent provoquer un échec fonctionnel. La bonne
approche est donc de détecter, diagnostiquer et isoler chaque incompatibilité,
plutôt que de masquer toutes les erreurs avec des valeurs arbitraires.
