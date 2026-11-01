# Ce qu'il reste pour faire tourner presque toutes les applications Windows

État des lieux du 1er novembre 2026, fait sur `master` (9669442). Ce document
répond à une question simple : **que faut-il encore écrire pour que la
quasi-totalité des logiciels Windows démarrent et fonctionnent sur Winlux ?**

Il complète [COMPATIBILITY.md](../COMPATIBILITY.md), qui décrit API par API ce
qui existe, et ne remplace aucun des suivis existants : chaque manque renvoie à
son issue GitHub quand elle existe déjà.

Issues des grands chantiers : #14 et #26 (fonctions et DLL système), #232
(32 bits), #233 (OpenGL et Vulkan), #234 (System32 et redistribuables), #235
(HTTPS), #137 (shaders Direct3D), #236 (Chromium/Electron), #22 et #138
(.NET bureau), #16, #19 et #20 (processus et COM), #237 (anciens DirectX),
#179 (installeurs).

Règle du projet rappelée : tout est maison. Pas de Wine, de Proton, de DXVK ni
d'aucune couche de compatibilité tierce. La plomberie Linux standard reste
permise (libGL/libvulkan de l'hôte, FFmpeg, curl, OpenSSL/GnuTLS, PipeWire…).

## En bref

- Winlux expose aujourd'hui **2 720 fonctions** réparties dans **57 DLL
  intégrées**. Windows 11 en exporte plusieurs dizaines de milliers ; les
  logiciels courants en appellent quelques centaines à quelques milliers.
- Mesuré sur **21 vrais logiciels Windows** (1 200 fichiers PE), **aucun** n'a
  aujourd'hui tous ses imports satisfaits. Il manque entre 3 fonctions (esbuild,
  écrit en Go) et 800 (pywin32) par logiciel, et 14 des 21 importent au moins
  une DLL que Winlux ne fournit pas du tout.
- Une fonction manquante n'empêche pas le démarrage, mais **le processus
  s'arrête au premier appel** (`ENTRYPOINT_NOT_FOUND`, voir
  `runtime/src/thunks.rs`). Une DLL manquante en import direct empêche le
  démarrage.
- Les **programmes 32 bits sont refusés** (`runtime/src/pe.rs`). C'est le plus
  gros trou structurel : une grande partie des logiciels anciens, des jeux
  d'avant 2015 et des installeurs sont en 32 bits.
- Les **shaders Direct3D 10/11/12 ne sont pas traduits** (#137) : tout jeu
  récent s'affiche faux, et les interfaces qui dessinent en Direct3D 11 (Qt
  Quick, Chromium/Electron, Flutter) aussi.
- **OpenGL (`opengl32.dll`, WGL) et Vulkan (`vulkan-1.dll`) n'existent pas**
  côté Windows. C'est le chantier graphique le moins cher pour le plus grand
  gain (jeux indépendants, SDL, raylib, Java, Blender, émulateurs…).

## Comment c'est mesuré

1. `winlux --exports` (ajouté par ce travail) liste chaque fonction exportée
   par les DLL intégrées.
2. `scripts/compat-imports.py` lit les imports de tous les .exe/.dll d'un
   logiciel décompressé avec `pefile`, applique les mêmes règles de nom que le
   chargeur (`dlls::normalize`, API-sets, `kernelbase` → `kernel32`…) et compte :
   les DLL absentes (ni intégrées ni livrées avec le logiciel), les imports
   différés absents et les fonctions manquantes dans les DLL intégrées.
3. Le corpus vient des dépôts publics joignables depuis la CI (PyPI, npm) :
   Qt 6.12 complet avec QtWebEngine (Chromium), Java 25 (jdk4py), Node.js 24,
   .NET Core 3.1, SDL2/pygame (64 et 32 bits), wxWidgets 3.3, raylib, panda3d,
   ffmpeg (deux builds), libvips (sharp), numpy/OpenBLAS (64 et 32 bits),
   pywin32/MFC, esbuild (Go), 7-Zip, les chargeurs PyInstaller (64 et 32 bits)
   et Squirrel (installeur d'Electron).

Le résultat brut, logiciel par logiciel, est dans
[compat/mesure-2026-11-01.md](compat/mesure-2026-11-01.md). Pour refaire la
mesure sur n'importe quel logiciel :

```sh
winlux --exports > exports.tsv
python3 scripts/compat-imports.py exports.tsv ~/logiciels/MonAppli/
```

Limites honnêtes de la mesure :

- elle est **statique** : elle dit ce qui manque pour charger, pas si ce qui
  existe se comporte bien. Une partie des 2 720 fonctions sont des bouchons
  (`user32_ext.rs` le dit lui-même) ;
- les gros logiciels propriétaires (Office, Photoshop, Steam, jeux AAA,
  Discord) ne sont pas téléchargeables depuis la CI ; leurs besoins sont déduits
  de leurs briques (Chromium, Qt, Direct3D) qui, elles, ont été mesurées ;
- les applis Electron ont les mêmes besoins que QtWebEngine (le même Chromium),
  c'est donc lui qui sert d'étalon.

## Ce qui existe aujourd'hui

| Domaine | État | Ce qui manque surtout |
|---|---|---|
| Chargeur PE | PE32+ x64, relocations, TLS, `DllMain`, imports différés, DLL réelles livrées avec l'appli | 32 bits, ARM64, dossier `System32`, déchargement (#24) |
| Noyau Win32 (`kernel32`, `ntdll`) | 508 + 108 fonctions : fichiers, mémoire, threads, synchro, IOCP partiels, SEH x64 | consoles avancées, Toolhelp, jobs, ports série, `DeviceIoControl`, mémoire d'autres processus (#14, #21) |
| CRT (`msvcrt`, `ucrtbase`) | 421 fonctions | `longjmp`/`setjmp`, `__CxxFrameHandler4`, RTTI, maths C99, variantes `_w*` (#14) |
| Fenêtres et GDI (`user32`, `gdi32`) | 287 + 154 fonctions, backend X11, Xft | rectangles, icônes, fenêtres en couches, régions, chemins GDI, transformations, entrée brute, écrans |
| Contrôles et shell | `comctl32` (19), `shell32` (19), `comdlg32` (7), `uxtheme` | ListView/TreeView complets, `SHFileOperation`, `SHGetFileInfo`, dialogues fichiers W |
| COM/OLE | appartements, `IStream`, fabriques en processus | activation depuis le registre, hors processus, automatisation (#16) |
| .NET | CLR maison (interpréteur CIL) ; coreclr embarqué par l'appli lancé tel quel | génériques BCL, délégués, LINQ, WinForms, WPF (#22, #138) |
| Direct3D | D3D9 (shaders SM1–3 traduits), D3D11/12 sur GL/Vulkan, DXGI, DXVA2 H.264 | shaders DXBC/DXIL (#137), D3D10, D3D8, DirectDraw, `d3dcompiler_47` |
| 2D et images | Direct2D, DirectWrite, WIC (COM) | GDI+ (`gdiplus.dll`), `msimg32`, Uniscribe (`usp10`) |
| OpenGL / Vulkan | uniquement en interne (backends de D3D) | `opengl32.dll` + WGL, `vulkan-1.dll` pour les applis |
| Son | waveOut, DirectSound, WASAPI, XAudio2 sur un mixeur PipeWire/Pulse/ALSA | MIDI (`midiOut*`, `midiIn*`), `msacm32` |
| Vidéo | MP3/AAC décodés, DXVA2 H.264 | chaîne Media Foundation (#23), DirectShow, `avicap32`, `evr` |
| Réseau | Winsock TCP/UDP de base, WinHTTP synchrone via curl, iphlpapi | `WSARecvFrom/WSASendTo`, `mswsock`, `dnsapi`, WinINet, `urlmon`, SSPI/Schannel |
| Sécurité et crypto | bcrypt, une partie de crypt32, jetons simplifiés | CryptoAPI ancienne (`CryptAcquireContext`…), `ncrypt`, magasins de certificats, ACL réelles |
| Impression | winspool → PDF → CUPS (IPP) | pilotes, XPS, polices de l'appli dans le PDF |
| Installeurs | NSIS, Inno Setup, MSI maison | plugins NSIS, actions MSI en DLL, WiX Burn, InstallShield (#179) |
| Entrées | XInput, DirectInput 8, clavier/souris | entrée brute (`RegisterRawInputDevices`), HID, IME (`imm32`) |

## Les manques, du plus bloquant au moins bloquant

L'ampleur est donnée en tailles de chantier : **S** = une PR, **M** = quelques
PR, **L** = plusieurs semaines de travail suivi, **XL** = plusieurs mois.

### 1. Combler les fonctions Win32 de base manquantes — bloque tout le monde

Chaque logiciel mesuré tombe sur des fonctions absentes de `kernel32`,
`user32`, `gdi32`, `advapi32` et de la CRT, et s'arrête au premier appel. Les
plus demandées (nombre de logiciels du corpus qui les importent) :

- `kernel32` : `DeviceIoControl` (14), `SetStdHandle` (12), `ReadConsoleW`
  (12), `SuspendThread` (11), `MulDiv` (11), `GetThreadContext` (10),
  `GetDiskFreeSpaceW` (9), `DebugBreak` (8), `CreateToolhelp32Snapshot` (8),
  `K32EnumProcessModules` (8), `OpenFileMappingW`, `LockFileEx` (7),
  `RegisterWaitForSingleObject` (6), `QueryFullProcessImageNameW` (5)…
- `user32` : `PtInRect`, `CreateIconIndirect`, `EnumDisplaySettingsW`,
  `SetLayeredWindowAttributes` (10 chacun), `TrackMouseEvent`,
  `PostThreadMessageW`, `SetWindowRgn`, `GetRawInputDeviceList` (9),
  `WindowFromPoint`, `FlashWindowEx`, `RegisterRawInputDevices`,
  `DialogBoxIndirectParamW` (8)…
- `gdi32` : `ChoosePixelFormat`, `SetPixelFormat`, `DescribePixelFormat`,
  `SwapBuffers` (10, c'est OpenGL, voir point 4), chemins (`BeginPath`,
  `StrokePath`…), transformations du monde, `GetFontData` (5).
- CRT : `__std_type_info_destroy_list` (11), `longjmp` (9),
  `__CxxFrameHandler4` (7, déjà repéré pour Paint.NET), `_purecall`,
  `__intrinsic_setjmp`, `_fdopen`, `_open_osfhandle`, RTTI (`__RTDynamicCast`),
  maths C99 (`exp2`, `log1p`, `fmin`…).
- `winmm` : `timeBeginPeriod`/`timeEndPeriod` (9, quasiment tous les jeux) et
  toute l'API MIDI (5).

Ampleur : **L**, mais mécanique et très rentable. 1 834 fonctions
distinctes manquent sur ce seul corpus ; les 300 premières couvrent l'essentiel
des appels. Suivi : #14 (kernel32/msvcrt/ntdll) et #26 (DLL système et mesure).
La mesure est à relancer après chaque lot.

### 2. Fournir les DLL système qui manquent entièrement — empêchent le démarrage

Une DLL absente en import direct et le programme ne se lance pas du tout.
Classement par nombre de logiciels touchés :

| DLL | Logiciels | Rôle | Ampleur |
|---|---|---|---|
| `imm32` | 9 / 21 | saisie de texte (IME) ; importée par Qt, SDL, wx, Java, ffmpeg, Chromium | S pour les entrées de base, M avec la vraie composition (lien #183) |
| `secur32` / `sspicli` | 4 (+1 différé) | SSPI : TLS Schannel, NTLM/Kerberos | L (voir point 6) |
| `msvcp140` | 4 | bibliothèque C++ de Visual Studio (voir point 5) | M |
| `psapi` | 3 | liste des processus et modules | S (alias des `K32*` de kernel32) |
| `ncrypt` | 3 | clés CNG | M |
| `netapi32`, `wtsapi32`, `pdh`, `urlmon` | 3 chacune | comptes/partages, sessions, compteurs de performance, URL Moniker | S à M chacune |
| `avicap32` | 3 | webcam ancienne API (ffmpeg) | S, sur `mfcapture`/V4L2 existants |
| `usp10`, `msimg32`, `gdiplus`, `oleacc` | 1 à 2 | Uniscribe, `AlphaBlend`/`TransparentBlt`, GDI+, accessibilité | S (`msimg32`) à L (`gdiplus`) |
| `opengl32` | 2 | OpenGL (voir point 4) | M |
| `dcomp`, `api-ms-win-shcore-scaling` | 2 | DirectComposition, DPI par écran | M / S |
| `cfgmgr32`, `hid`, `winusb`, `bthprops.cpl` | 2 | périphériques USB/HID/Bluetooth | M |
| `mswsock`, `wsock32`, `dnsapi`, `dhcpcsvc` | 1 à 2 | extensions Winsock, DNS | S |
| `cabinet`, `msi`, `mfc42`/`mfc140u`, `lz32` | 1 à 2 | archives CAB, MSI, MFC | M |
| `uiautomationcore`, `mscms`, `fontsub`, `winscard`, `evr`, `credui`, `wininet` | 1 chacune | UI Automation, profils couleur, sous-ensembles de polices, cartes à puce, rendu vidéo, identifiants, Internet | S à L |

Ampleur globale : **L**. Beaucoup sont petites (alias, quelques fonctions) ;
`gdiplus`, `secur32`, `wininet` et `uiautomationcore` sont de vrais chantiers.
Suivi : #26.

### 3. Programmes 32 bits — un logiciel ancien sur deux

`pe.rs` refuse toute image PE32 avec du code natif : 7-Zip 32 bits, les
installeurs et la plupart des jeux d'avant 2015 s'arrêtent avant la première
instruction. Dans le corpus, l'installeur Squirrel d'Electron (19 fichiers sur
21) et les variantes win32 de SDL et numpy sont dans ce cas.

Ce qu'il faut : un runtime i686 (le binaire `winlux` compilé pour
`i686-unknown-linux-gnu`, que le noyau Linux sait exécuter), un chargeur PE32,
une variante `stdcall`/`cdecl` de chaque fonction exportée (aujourd'hui toutes
en `extern "win64"`), le TEB 32 bits sur `fs:`, le SEH 32 bits par chaîne de
cadres, un espace d'adressage sous 2/4 Go et le lancement croisé 32 ↔ 64 bits
par `CreateProcess`. Les bibliothèques Linux i386 (X11, son, GL) doivent entrer
dans l'ISO.

Ampleur : **XL**, mais en grande partie automatisable : les exports sont
déclarés par une macro, qui peut générer les deux conventions. Suivi : #232.

### 4. OpenGL et Vulkan pour les applications — le gain le moins cher

Winlux s'en sert en interne pour Direct3D, mais un programme qui importe
`opengl32.dll` ou `vulkan-1.dll` ne démarre pas. Sont concernés : les jeux SDL,
raylib, GLFW, panda3d, Godot, Minecraft (Java), Blender, les émulateurs, les
logiciels de CAO, Qt en mode OpenGL. Dix logiciels du corpus importent déjà les
fonctions WGL de `gdi32` (`ChoosePixelFormat`, `SwapBuffers`).

Ce qu'il faut : `opengl32.dll` qui transmet les appels à la libGL de l'hôte,
les contextes WGL (`wglCreateContext`, `wglMakeCurrent`,
`wglGetProcAddress`, extensions `WGL_ARB_*`) reliés à GLX/EGL sur la fenêtre
X11 du HWND ; `vulkan-1.dll` qui transmet au chargeur Vulkan de l'hôte et
traduit `VK_KHR_win32_surface` en surface Xlib. Les conventions d'appel
changent (Windows x64 vers System V), il faut donc des thunks générés.

Ampleur : **M** pour chacun. Suivi : #233.

### 5. Bibliothèque C++ de Visual Studio et dossier System32

`msvcp140.dll`, `mfc140u.dll`, `concrt140.dll`, `vcomp140.dll` et
`d3dcompiler_47.dll` sont normalement installés par le « redistribuable » de
Visual Studio ou de DirectX. Beaucoup d'applis supposent qu'ils sont déjà là
(wxPython : 22 modules en dépendent). Winlux ne les fournit pas et son
chargeur ne cherche que dans le dossier de l'appli, le dossier courant et
`PATH` : il n'existe pas de `C:\Windows\System32`.

Ce qu'il faut : un dossier système où les installeurs peuvent déposer leurs
DLL (et que le chargeur parcourt), le support des installeurs WiX Burn
(`vc_redist.x64.exe` en est un, lien #179), puis, pour les machines sans
redistribuable, une `msvcp140` maison (conteneurs et flux C++ sur la CRT
existante).

Ampleur : **S** pour le dossier système, **M** pour Burn, **L** pour une
`msvcp140` complète. Suivi : #234.

### 6. HTTPS et sécurité des applications

Le réseau de base fonctionne, mais tout ce qui chiffre passe par des API
absentes : SSPI/Schannel (`secur32`, Java, Qt Network, pywin32), CNG `ncrypt`,
la CryptoAPI ancienne (`CryptAcquireContextW`, `CryptCreateHash`… dans 6 à 7
logiciels), les magasins de certificats de `crypt32`
(`CertGetCertificateContextProperty`, 7) et WinINet/urlmon (téléchargements,
mises à jour automatiques). Sans eux : pas de connexion à un compte, pas de
mise à jour intégrée, pas d'API HTTPS pour les applis qui n'embarquent pas leur
propre OpenSSL.

Ce qu'il faut : Schannel sur la bibliothèque TLS de l'hôte (plomberie Linux,
comme le curl de `winhttp`), les magasins de certificats branchés sur
`/etc/ssl/certs`, CryptoAPI et `ncrypt` sur le moteur `bcrypt` existant,
WinINet sur le même moteur que WinHTTP.

Ampleur : **L**. Suivi : #235.

### 7. Shaders Direct3D 10/11/12 — tous les jeux récents

Déjà suivi par #137. Sans traduction DXBC (SM4/5) et DXIL (SM6) vers GLSL ou
SPIR-V, les dessins utilisent des shaders passe-plat : l'image est fausse dans
tout jeu Direct3D 11 ou 12. Cela touche aussi les interfaces qui dessinent en
Direct3D 11 : Qt Quick, Chromium (via ANGLE), Flutter, WinUI. Il faut aussi
`d3dcompiler_47.dll` (compilation HLSL à la volée, importé par Qt et de
nombreux moteurs), Direct3D 10, et pour les anciens jeux Direct3D 8 et
DirectDraw (point 12).

Ampleur : **XL** (deux traducteurs de bytecode complets, le plus gros chantier
graphique). Suivi : #137.

### 8. Chromium, Electron, CEF et WebView2 — une grande partie des applis modernes

Discord, Slack, Teams, Spotify, Notion, Obsidian, WhatsApp, Steam (son
interface), les lanceurs de jeux et une foule d'applis professionnelles sont
des applis Electron ou CEF. Mesuré sur QtWebEngine (le même Chromium) : 26 DLL
absentes et plus de 360 fonctions manquantes.

Ce qu'il faut, en plus des points 1, 2, 6 et 7 : le modèle multi-processus de
Chromium (objets nommés et mémoire partagée entre processus, tubes nommés,
`DuplicateHandle` vers un autre processus, objets Job, jetons restreints pour
le bac à sable), DirectComposition (`dcomp`) et le rendu D3D11 ou son repli
logiciel SwiftShader. WebView2 dépend d'Edge, produit Microsoft exclu par la
règle du projet : il faudra une implémentation maison des interfaces
`ICoreWebView2` sur le moteur web de l'hôte.

Ampleur : **XL**. Suivi : #236, qui s'appuie sur #19 et #20.

### 9. Applications .NET de bureau (WinForms, WPF)

Les applis .NET autonomes qui embarquent `coreclr.dll` démarrent par leur
propre runtime ; Paint.NET avance sur ce chemin (#138). Les applis qui
s'appuient sur un .NET installé passent par le CLR maison, qui n'a encore ni
génériques de la BCL, ni délégués, ni LINQ, ni WinForms, ni WPF. WPF demande en
plus Direct3D 9 (déjà là) et ses DLL natives (`wpfgfx`, `PresentationNative`).
Le corpus .NET Core 3.1 montre 118 fonctions natives manquantes, dont une
partie dans `ole32`/`oleaut32`.

Ampleur : **XL**. Suivi : #22, #138.

### 10. Communication entre processus et COM complet

Objets nommés partagés entre processus, tubes nommés, héritage de handles,
fenêtres d'autres processus (`FindWindow`, `SendMessage` d'une appli à une
autre), serveurs COM hors processus et activation par le registre. Ce sont les
fondations des lanceurs, des suites bureautiques, des extensions, des
compléments Office et de l'automatisation.

Ampleur : **L**. Suivi : #16, #17, #19, #20.

### 11. Périphériques et entrées des jeux

`DeviceIoControl` (le plus demandé du corpus), entrée brute
(`RegisterRawInputDevices`, `GetRawInputData` : souris des jeux de tir), HID et
`setupapi`/`cfgmgr32` (`CM_Get_Parent`…, manettes et périphériques USB), ports
série (`SetCommState`…, 4 logiciels), `SetDeviceGammaRamp`, changement de
résolution (`ChangeDisplaySettingsExW`, 8).

Ampleur : **M**. À ranger dans #14/#26.

### 12. Anciens DirectX et multimédia

DirectDraw, Direct3D 8, `d3dx9_*`, DirectPlay, DirectShow (aujourd'hui
`quartz` ne fait que renvoyer des erreurs), la chaîne Media Foundation de
lecture (#23), MIDI, `msacm32`. Ce sont les jeux et lecteurs des années
2000–2015, presque toujours en 32 bits (point 3).

Ampleur : **L**. Suivi : #237, et #23 pour la vidéo.

### 13. Installeurs restants

NSIS, Inno Setup et MSI marchent ; il reste les plugins NSIS
(`System::Call`), les actions personnalisées MSI en DLL/EXE, WiX Burn,
InstallShield et les installeurs 32 bits (point 3).

Ampleur : **M** à **L**. Suivi : #179.

## Hors de portée ou exclu

- **Anti-triche en mode noyau** (Easy Anti-Cheat noyau, BattlEye, Vanguard,
  FACEIT) et **pilotes Windows** (`.sys` : VPN, imprimantes, matériel) : ils
  exigent le noyau Windows. Les jeux en ligne qui les imposent ne tourneront
  pas, sauf si l'éditeur active le mode Linux de son anti-triche.
- **Antivirus** : exclu par choix du projet.
- **Applis du Microsoft Store (UWP/MSIX, WinRT, AppContainer)** : possibles à
  très long terme, mais pas un objectif tant que le bureau Win32 n'est pas
  couvert ; et le Store lui-même est un produit Microsoft.
- **Exécutables ARM64** : marginaux sur PC, à reconsidérer plus tard.

## Ce que chaque palier débloque

| Palier atteint | Ce qui tourne |
|---|---|
| Points 1 + 2 (petites DLL) + 5 (dossier système) | outils en ligne de commande et utilitaires (Go, Rust, Node.js, ffmpeg, 7-Zip 64 bits, Python), petits logiciels Win32 classiques |
| + 4 (OpenGL/Vulkan) + entrée brute | jeux indépendants SDL/GLFW/raylib/Godot, émulateurs, Java avec OpenGL, Blender |
| + `imm32`, `usp10`, `gdiplus`, `msimg32`, contrôles communs complets | bureautique et outils Qt, wxWidgets, MFC (Notepad++, éditeurs, gestionnaires de fichiers, clients FTP) |
| + 6 (HTTPS) | logiciels avec compte en ligne et mises à jour intégrées |
| + 3 (32 bits) | logiciels anciens, la moitié des installeurs, jeux 32 bits OpenGL/D3D9 |
| + 7 (shaders D3D11/12) | jeux récents sans anti-triche noyau, interfaces Qt Quick/Flutter accélérées |
| + 8 (Chromium) + 10 | applis Electron et CEF : Discord, Slack, Spotify, Notion, lanceurs |
| + 9 (.NET bureau) | applis WinForms/WPF, Paint.NET |

À ce stade, la grande majorité des logiciels de bureau et des jeux sans
anti-triche noyau seraient couverts. Le reste (pilotes, anti-triche noyau,
Store) dépend de choix extérieurs au projet.

## Ordre conseillé

1. Les petites DLL du point 2 et les 300 fonctions les plus demandées du
   point 1, par lots, avec la mesure relancée après chaque lot.
2. OpenGL et Vulkan (point 4) : peu de code, beaucoup de jeux.
3. Dossier système et redistribuables (point 5).
4. HTTPS (point 6), en parallèle des shaders (point 7, #137) déjà commencés.
5. Le 32 bits (point 3), dès que la macro d'exports sait produire les deux
   conventions.
6. Chromium (point 8), puis .NET bureau (point 9) qui avance déjà avec
   Paint.NET.
