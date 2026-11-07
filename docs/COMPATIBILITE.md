# Ce qu'il reste pour faire tourner presque toutes les applications Windows

État des lieux du 1er novembre 2026, fait sur `master` (9669442). Ce document
répond à une question simple : **que faut-il encore écrire pour que la
quasi-totalité des logiciels Windows démarrent et fonctionnent sur Winlux ?**

> **Mise à jour du 2 novembre 2026** ([compat/mesure-2026-11-02.md](compat/mesure-2026-11-02.md)) :
> le premier manque n'est pas une DLL. Les boîtes de dialogue, les contrôles
> standard (boutons, champs, listes) et les fenêtres enfants ne sont pas
> encore implémentés : les logiciels Win32 classiques démarrent sans rien
> afficher. C'est le chantier prioritaire, devant tous ceux listés ci-dessous.

Il complète [COMPATIBILITY.md](../COMPATIBILITY.md), qui décrit API par API ce
qui existe, et ne remplace aucun des suivis existants : chaque manque renvoie à
son issue GitHub quand elle existe déjà.

Issues des grands chantiers : #14 et #26 (fonctions et DLL système), #232
(32 bits), #233 (OpenGL et Vulkan), #234 (System32 et redistribuables), #235
(HTTPS), #137 (shaders Direct3D), #236 (Chromium/Electron), #22 et #138
(.NET bureau), #16, #19 et #20 (processus et COM), #237 (anciens DirectX),
#179 (installeurs), et depuis le 2 novembre #269 (fautes matérielles en
exceptions, suspension de threads), #270 (`ReadDirectoryChangesW`, ConPTY),
#271 (périphériques), #272 (MSIX) et #265 à #268 (suite de Paint.NET).

Depuis #269 (première partie), les fautes matérielles du code d'un programme
(accès mémoire invalide, division par zéro, `int3`, `ud2`, pas à pas,
instruction privilégiée, page de garde `PAGE_GUARD`, débordement de pile)
deviennent des exceptions Windows que ses gestionnaires vectorisés et ses
blocs `__try/__except` rattrapent (`runtime/src/dlls/seh_hw.rs`, test
`scripts/test-hw-exceptions.sh`). `WINLUX_SEH_HW=0` rend l'ancien comportement.

Depuis #272, les paquets MSIX et AppX (`.msix`, `.appx`, `.msixbundle`) s'installent
par double-clic ou `winlux --setup paquet.msix` (`runtime/src/setup/msix.rs`) :
empreintes du block map vérifiées avant toute écriture, installation dans
`C:\Program Files\WindowsApps\<PackageFullName>`, menu Démarrer, associations et
protocoles, `LocalState`, identité de paquet à l'exécution
(`GetCurrentPackageFullName`, `GetCurrentPackageId`, `GetCurrentPackagePath`…).
Éditeur : comme Windows, un paquet non signé n'est pas installé sans
avertissement. Il est refusé par défaut avec « Éditeur non vérifié » (code 3), et le
double-clic demande une confirmation explicite (« Annuler » par défaut,
« Installer quand même ») avant de relancer avec `--allow-unverified`. Un paquet signé
est vérifié entièrement (#235, `runtime/src/setup/appx_signature.rs`, calculs par
l'OpenSSL du système) : signature RSA ou ECDSA des attributs signés, `messageDigest`
du `SpcIndirectDataContent`, condensats `AXBM` (block map, qui protège chaque
fichier), `AXCT`, `AXCI`, `AXPC` et `AXCD` du paquet reçu, usage « signature de
code » du certificat et chaîne jusqu'à une autorité du système. Entièrement vérifié,
il s'installe sans question ; signé par une autorité inconnue (certificat de test,
autorité d'entreprise non installée) ou expiré, il passe par la même confirmation ;
une signature fausse, au nom d'un autre éditeur, ou un paquet modifié après
signature est refusé dans tous les cas.
Pas encore : la contre-signature d'horodatage (un certificat expiré reste à confirmer),
la révocation en ligne, les autorités de signature de code propres à Windows (absentes du
magasin du système : un paquet du Store reste donc à confirmer), les paquets de ressources des bundles, et les paquets 32 bits (#232).
Test : `scripts/test-msix.sh`.

Depuis #271 (première partie), les manettes, volants et joysticks se lisent
aussi en HID brut, comme le font SDL, les moteurs de jeu et les outils de
configuration : `setupapi` énumère `GUID_DEVINTERFACE_HID` depuis
`/sys/class/hidraw`, `CreateFileW` ouvre le chemin `\\?\hid#vid_…#hidrawN#{…}`,
et `hid.dll` rend attributs, noms, capacités (`HidP_GetCaps`, tirées du
descripteur) et rapports de fonction (`runtime/src/dlls/hid.rs`). Les lectures
et écritures, OVERLAPPED compris, passent par un relais qui ajoute ou retire
l'octet d'identifiant de rapport que Windows impose et que hidraw omet. La règle
`integration/70-winlux-manettes.rules` ouvre les `/dev/hidraw*` des fabricants
de manettes au groupe `input`. Test : `scripts/test-hid.sh`.

`DeviceIoControl` (#271) répond aux codes de disques, de volumes et de fichiers
(`runtime/src/dlls/devio.rs`) : `\\.\C:` et `\\.\PhysicalDriveN` s'ouvrent
sans toucher au vrai périphérique, et `IOCTL_STORAGE_QUERY_PROPERTY` (fabricant,
modèle, numéro de série, bus, SSD ou non, TRIM, alignement),
`IOCTL_DISK_GET_DRIVE_GEOMETRY(_EX)`, `GET_LENGTH_INFO`, `GET_PARTITION_INFO_EX`,
`IOCTL_STORAGE_GET_DEVICE_NUMBER`, `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` sont lus
dans /sys/block. Côté fichiers : `FSCTL_SET_SPARSE`, `SET_ZERO_DATA` (trou
percé), `QUERY_ALLOCATED_RANGES` (SEEK_DATA/SEEK_HOLE), compression et
identifiant d'objet. Les rapports HID passent aussi par `IOCTL_HID_*`.
Verrouiller ou démonter un volume est refusé (accès refusé), tout autre code
rend `ERROR_NOT_SUPPORTED`. Test : `scripts/test-devio.sh`.

L'entrée brute (#271, `runtime/src/dlls/rawinput.rs`) donne aux jeux de tir la
souris en déplacements relatifs, sans accélération ni blocage au bord de
l'écran : `RegisterRawInputDevices` lance un fil qui lit les souris et claviers
evdev de l'hôte (`/dev/input/event*`, ouverts au groupe `input`) et poste
`WM_INPUT` à la fenêtre inscrite tant que le programme a le focus (toujours avec
`RIDEV_INPUTSINK`). `GetRawInputData`, `GetRawInputBuffer`,
`GetRawInputDeviceList` et `GetRawInputDeviceInfoA/W` (nom, fiche, manettes HID)
suivent. Boutons 1 à 5, molettes (haute résolution comprise), scancodes PS/2 avec
le préfixe E0. Test : `scripts/test-rawinput.sh` (fausse souris et faux clavier
sur des FIFO, `WINLUX_RAWINPUT_DEVICES`).

Les ports série (`COMn` sur `/dev/ttyS*`, `/dev/ttyUSB*`, `/dev/ttyACM*`, DCB et
délais sur termios) se trouvent désormais comme sous Windows (#271,
`runtime/src/dlls/serial.rs`) : `QueryDosDeviceW/A` liste les lecteurs et les
ports présents, `HKLM\HARDWARE\DEVICEMAP\SERIALCOMM` est rafraîchie à chaque
ouverture, et `SetCommMask`/`WaitCommEvent` (OVERLAPPED compris),
`GetCommProperties`, `BuildCommDCBA/W`, `TransmitCommChar`, `Get/SetCommConfig`
et `GetDefaultCommConfig` complètent. Test : `scripts/test-serial.sh` (COM7 sur
un pseudo-terminal qui répond).

Les jeux en plein écran exclusif changent la définition et la rampe gamma
(#271, `runtime/src/dlls/displaymode.rs`) : `EnumDisplaySettings(Ex)A/W` liste les
vrais modes et fréquences de chaque écran (`xrandr --query`, `\\.\DISPLAY1` =
écran principal), `ChangeDisplaySettings(Ex)A/W` applique la demande par
`xrandr --output … --mode … --rate …` (CDS_TEST, DISP_CHANGE_BADMODE,
WM_DISPLAYCHANGE), et `SetDeviceGammaRamp`/`GetDeviceGammaRamp` passent par la
rampe RandR de la CRTC. Tout est défait à la fin du programme, même s'il
plante : un gardien `winlux --display-restore`, lancé au premier changement avec
l'état d'origine, attend la fermeture de son entrée standard puis remet l'écran.
Rien n'est écrit dans la configuration d'écran mémorisée. Test :
`scripts/test-display-mode.sh` (faux xrandr). `GetDIBits` décrit l'écran comme
Windows (32 bits, `BI_BITFIELDS`, 8 bits par composante) : SDL3 en tire le
format des modes, que Warzone 2100 écartait tous.

Les ports COM se trouvent aussi comme le fait pyserial, l'IDE Arduino ou un
logiciel de flashage (#271, `runtime/src/dlls/devnodes.rs`). Un arbre de
périphériques Plug and Play est construit à partir de sysfs : la racine, les
ports (`USB\VID_…&PID_…&MI_…` sous leur appareil USB `USB\VID_…&PID_…\<série>`,
`ACPI\PNP0501`, `ROOT\PORTS`) et les manettes HID. `setupapi` sert la classe Ports et
l'interface COMPORT (`SetupDiOpenDevRegKey` → `PortName`,
`SetupDiGetDeviceInstanceId`, `FRIENDLYNAME` « … (COM5) », `HARDWAREID`,
`SetupDiClassGuidsFromName`). `cfgmgr32` parcourt l'arbre (`CM_Locate_DevNode`,
`CM_Get_Parent/Child/Sibling`, `CM_Get_Device_ID`, `CM_Get_DevNode_Status`,
`CM_Get_DevNode_Registry_Property`, `CM_Get_Device_Interface_List`). Un chemin
d'interface COMPORT s'ouvre avec `CreateFileW` comme le port lui-même. Test :
`scripts/test-devnodes.sh` (faux sysfs d'une Arduino Uno, plus COM7 sur un
pseudo-terminal).

L'accès USB direct des outils de flashage, de radio logicielle et de libusb
passe par `winusb.dll` (#271, `runtime/src/dlls/winusb.rs`). Les appareils de
`/sys/bus/usb/devices` sont listés par setupapi sous `GUID_DEVINTERFACE_USB_DEVICE`,
puis `CreateFileW` sur leur chemin ouvre `/dev/bus/usb/BBB/DDD` (usbfs).
`WinUsb_Initialize` réclame l'interface sans jamais détacher un pilote du noyau
(ERROR_BUSY sinon). Fonctions servies : descripteurs, vitesse, réglages
d'interface et points de terminaison (`QueryInterfaceSettings`, `QueryPipe`),
`ControlTransfer`, `ReadPipe`/`WritePipe` (bulk et interruption), délais par
`PIPE_TRANSFER_TIMEOUT`, `ResetPipe`, réglage alternatif, interfaces associées.
Sans droit sur usbfs, la description vient du fichier `descriptors` de sysfs et
les transferts échouent proprement (ERROR_ACCESS_DENIED). L'isochrone n'est pas
pris en charge. Test : `scripts/test-winusb.sh` (faux sysfs).

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
| Direct3D | D3D8 (traduit sur D3D9), D3D9 (shaders SM1–3 traduits), D3D11/12 sur GL/Vulkan, DirectDraw 1–7, DXGI, DXVA2 H.264 | shaders DXBC/DXIL (#137), D3D10, `d3dx8`/`d3dx9`, `d3dcompiler_47` |
| 2D et images | Direct2D, DirectWrite, WIC (COM) | GDI+ (`gdiplus.dll`), `msimg32`, Uniscribe (`usp10`) |
| OpenGL / Vulkan | uniquement en interne (backends de D3D) | `opengl32.dll` + WGL, `vulkan-1.dll` pour les applis |
| Son | waveOut, DirectSound, WASAPI, XAudio2, MIDI (`midiOut*`, `midiStream*` sur un synthétiseur General MIDI maison) sur un mixeur PipeWire/Pulse/ALSA | entrée MIDI (`midiIn*`), `msacm32` |
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

**Fait (novembre 2026).** `opengl32.dll` et `vulkan-1.dll` existent
(`runtime/src/dlls/opengl32.rs`, `vulkan1.rs`). Un pont générique
(`hostcall.rs`) traduit chaque appel Windows x64 en appel System V d'après la
signature de la fonction, tirée des registres Khronos (`sigs/gl.txt` : 3 295
fonctions OpenGL, `sigs/vk.txt` : 716 commandes Vulkan, regénérés par
`scripts/gen-host-signatures.py`). WGL est relié à GLX sur la fenêtre X11 du
HWND (formats de pixel, `WGL_ARB_create_context`, `WGL_ARB_pixel_format`,
`WGL_EXT_swap_control`), `ChoosePixelFormat` / `SwapBuffers` sont dans gdi32.
Côté Vulkan, `VK_KHR_win32_surface` devient une surface Xlib, les rappels de
débogage sont enveloppés et les allocateurs du programme remplacés par celui
du pilote. Vérifié par `gl_vk_suite.exe` (OpenGL 4.5 Mesa, Vulkan lavapipe).
Reste : `wglUseFontBitmaps` et le rendu dans un bitmap (`PFD_DRAW_TO_BITMAP`).

### 5. Bibliothèque C++ de Visual Studio et dossier System32

`msvcp140.dll`, `mfc140u.dll`, `concrt140.dll`, `vcomp140.dll` et
`d3dcompiler_47.dll` sont normalement installés par le « redistribuable » de
Visual Studio ou de DirectX. Beaucoup d'applis supposent qu'ils sont déjà là
(wxPython : 22 modules en dépendent). Winlux ne les fournit pas et son
chargeur ne cherchait que dans le dossier de l'appli, le dossier courant et
`PATH` : il n'existait pas de `C:\Windows\System32`.

Ce qu'il faut : un dossier système où les installeurs peuvent déposer leurs
DLL (et que le chargeur parcourt), le support des installeurs WiX Burn
(`vc_redist.x64.exe` en est un, lien #179), puis, pour les machines sans
redistribuable, une `msvcp140` maison (conteneurs et flux C++ sur la CRT
existante).

Ampleur : **S** pour le dossier système, **M** pour Burn, **L** pour une
`msvcp140` complète. Suivi : #234.

**Fait (novembre 2026).**
- `C:\Windows\System32` est parcouru par le chargeur juste après le dossier
  de l'appli, dans l'ordre de Windows (`loader::find_dll_file`).
- Les bundles WiX Burn s'installent sans exécuter leur moteur
  (`runtime/src/setup/burn.rs`) : Winlux lit la section `.wixburn`, extrait
  les conteneurs (cabinets), lit `BurnManifest.xml` et installe la chaîne.
  Les paquets MSI passent par le msiexec maison, les `.exe` sont relancés
  avec leurs arguments d'installation, `.msu`/`.msp` sont ignorés.
  `/uninstall` retire les paquets. Un MSI 32 bits dépose ses DLL système
  dans `SysWOW64`, comme sous Windows 64 bits. Les redistribuables
  arrivent donc avec les applis, après l'installation de Winlux, et ne sont
  pas dans l'ISO.
- Une `msvcp140` maison sert quand aucun redistribuable n'est installé :
  threads, mutex et variables de condition (compatibles avec les deux
  dispositions du STL), horloges, `random_device`, tables d'erreurs. Les
  `_X*` (erreurs fatales de conteneur) arrêtent le programme avec un
  message au lieu de lever l'exception C++. La vraie `msvcp140.dll`, si
  elle est présente, passe toujours avant (`dlls::prefers_real_file`).
- Vérifié par `system32_suite.exe` et par un bundle de test réel (MSI
  produit par wixl) dans les tests unitaires.
- Reste : flux et locale C++ (`iostream`), MFC, `concrt140`, `vcomp140`,
  et `d3dcompiler_47` (avec #137).

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

**Fait (novembre 2026).**
- WinINet (`InternetOpen`, `InternetOpenUrl`, `HttpOpenRequest`,
  `HttpSendRequest`, `HttpQueryInfo`…) passe par le moteur de WinHTTP ;
  urlmon fournit `URLDownloadToFile` et `URLDownloadToCacheFile`.
- `crypt32` donne de vrais certificats : `CERT_CONTEXT` et `CERT_INFO`
  remplis par un lecteur DER maison, magasin `ROOT` alimenté par les
  autorités de l'hôte (`/etc/ssl/certs/ca-certificates.crt`), magasins en
  mémoire, recherche (SHA-1, sujet, émetteur), propriétés de condensat,
  `CertGetNameString`, `CertNameToStr`, `CryptStringToBinary` et
  `CryptBinaryToString`. Les chaînes (`CertGetCertificateChain`) et la
  stratégie SSL (`CertVerifyCertificateChainPolicy`, nom du serveur compris)
  sont vérifiées par l'OpenSSL de l'hôte (`runtime/src/dlls/hostssl.rs`).
- Schannel (`runtime/src/dlls/schannel.rs`) : `AcquireCredentialsHandle`,
  `InitializeSecurityContext` (codes `SEC_I_CONTINUE_NEEDED`,
  `SEC_E_INCOMPLETE_MESSAGE`, tampons `EXTRA` et `MISSING` comme Windows),
  `EncryptMessage`, `DecryptMessage`, fermeture par `ApplyControlToken`,
  attributs (tailles du flux, certificat du serveur, protocole, ALPN) et la
  table `InitSecurityInterface`. Le certificat du serveur est validé
  automatiquement avec les autorités du système, sauf validation manuelle
  demandée par l'appli. Kerberos, NTLM et Negotiate restent absents.
- CryptoAPI : `CryptCreateHash`, `CryptHashData`, `CryptGetHashParam`,
  `CryptDuplicateHash` (MD4, MD5, SHA-1, SHA-2) ; `bcrypt` gagne MD5, SHA-384,
  SHA-512 et HMAC. Clés symétriques : `CryptImportKey` (`PLAINTEXTKEYBLOB`),
  `CryptGenKey`, `CryptDeriveKey`, `CryptEncrypt`/`CryptDecrypt` en CBC ou
  ECB avec bourrage PKCS#5 (DES, 3DES, AES 128/192/256, RC4), paramètres de
  clé (IV, mode, longueur). C'est ce que demande le NTLM de libcurl.
- Le chemin de libcurl avec un fichier d'autorités (`CURLOPT_CAINFO`) :
  `CryptQueryObject` (certificat binaire, PEM ou base64, tampon ou fichier),
  `CertCreateCertificateChainEngine` avec racines exclusives
  (`hExclusiveRoot`, chaîne partielle acceptée), `CertGetNameString` avec
  tous les noms DNS (`CERT_NAME_SEARCH_ALL_NAMES_FLAG`), `CryptDecodeObjectEx`
  pour les noms alternatifs et les contraintes de base.
- Vérifié par `tls_suite.exe` (poignée de main réelle contre un serveur
  `openssl s_server` jetable, `scripts/run-tls-suite.sh`) et par les tests
  unitaires.
- Données protégées (`runtime/src/dlls/dpapi.rs`) : `CryptProtectData`,
  `CryptUnprotectData` (AES-256-CBC et HMAC-SHA256, clé maîtresse par compte
  dans `~/.config/winlux/dpapi/master.key`, clé de la machine avec
  `CRYPTPROTECT_LOCAL_MACHINE`, entropie et description gérées),
  `CryptProtectMemory`/`CryptUnprotectMemory` et `RtlEncryptMemory`. Un blob
  venu d'un vrai Windows ne se relit pas : il dépend des secrets du compte
  Windows.
- Certificats client : `PFXImportCertStore` (PKCS #12 déchiffré par
  OpenSSL, clé privée attachée au certificat, `CERT_KEY_PROV_INFO_PROP_ID`),
  `PFXIsPFXBlob`, `PFXVerifyPassword`, `CERT_FIND_HAS_PRIVATE_KEY` ; Schannel
  présente le certificat de `SCHANNEL_CRED.paCred` au serveur qui le demande.
- Clés RSA de la CryptoAPI (`runtime/src/dlls/advapi32_rsa.rs`) : conteneurs
  de clés par compte (`CryptAcquireContext` avec `CRYPT_NEWKEYSET`,
  `CRYPT_DELETEKEYSET`, `CRYPT_VERIFYCONTEXT` ; clés gardées dans
  `~/.config/winlux/crypto/`, chiffrées par la DPAPI), `CryptGenKey`
  (`AT_KEYEXCHANGE`, `AT_SIGNATURE`), `CryptGetUserKey`, `CryptExportKey` et
  `CryptImportKey` (`PUBLICKEYBLOB`, `PRIVATEKEYBLOB`, `PLAINTEXTKEYBLOB`,
  `SIMPLEBLOB`), HMAC (`CALG_HMAC`, `HP_HMAC_INFO`, `CRYPT_IPSEC_HMAC_KEY`),
  `CryptSetHashParam(HP_HASHVAL)`,
  `CryptSignHash`/`CryptVerifySignature` (PKCS #1 v1.5, MD5, SHA-1, SHA-2),
  `CryptEncrypt`/`CryptDecrypt` RSA (PKCS #1 v1.5 ou `CRYPT_OAEP`),
  `CryptAcquireCertificatePrivateKey` (clé d'un certificat importé d'un
  PKCS #12) et `CryptImportPublicKeyInfo(Ex)`. Calculs par l'OpenSSL du
  système.
- Reste : `ncrypt`, `CALG_MAC` (MAC par chiffrement), rôle serveur de
  Schannel.

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

Fait (Direct3D 11, SM4/5) : `runtime/src/dlls/dxbc.rs` traduit le bytecode
DXBC en GLSL 4.30 (signatures, tampons de constantes, textures et
échantillonneurs, calcul flottant et entier, sauts et boucles, geometry
shaders). `d3d11_pipeline.rs` applique au tirage la mise en page des entrées,
les tampons de sommets, d'index et de constantes, les vues de textures, les
échantillonneurs, les états de mélange, de profondeur et de rastérisation, et
les ciseaux. Un périphérique sans chaîne d'échange rend hors écran. Test :
`scripts/test-d3d11-rendu.sh`, un .exe qui dessine un quad texturé sous Mesa
et relit ses pixels.

Reste : DXIL (SM6, Direct3D 12), compute shaders et UAV, sortie de flux,
hull et domain shaders, textures 3D, cubes et tableaux. Il reste aussi le HLSL
compilé à la volée : notre `D3DCompile` garde le texte dans un chunk « HLSL »
sans produire de DXBC, donc ANGLE (Chromium, Warzone 2100) n'en profite pas
encore.

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

**Fait (novembre 2026), pour crashpad (Chromium, Electron).** Héritage des
handles entre processus .exe (`runtime/src/dlls/inherit.rs`) : `bInheritHandle`
de `SECURITY_ATTRIBUTES`, `SetHandleInformation`/`GetHandleInformation`,
`DuplicateHandle(…, bInheritHandle)` (y compris un vrai handle vers le
processus courant) et `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`. L'enfant retrouve
chaque handle à la même valeur : fichiers, tubes et sockets en descripteurs
hérités, événements dans une page partagée attendue par futex, processus par
leur PID, instances de tube nommé reprises par l'enfant. Les tubes nommés se
joignent d'un processus à l'autre par leur nom (socket Unix de rendez-vous dans
`$XDG_RUNTIME_DIR/winlux-tubes`, droits 0700). Test : `scripts/test-heritage.sh`.
Pas encore : `DuplicateHandle` vers un **autre** processus (le courtier de
Chromium), la mémoire partagée (`CreateFileMapping`) et les mutex et sémaphores
hérités, l'écoute des clients d'autres processus avant `ConnectNamedPipe`, le
mode message des tubes entre processus. `crashpad_handler.exe` (Warzone 2100,
Sentry) reste non lancé tant qu'il n'a pas été essayé avec ces handles.

**Démarrage d'Electron (novembre 2026).** Relevé sur un vrai Electron 45 dans la
VM, chaque absence arrêtant Chromium (CHECK). Ajoutés
(`runtime/src/dlls/chromium_win32.rs`) :
- atténuations `Get/SetProcessMitigationPolicy`, lues comme désactivées ;
- `Get/SetProcessInformation`, `SetThreadInformation`, `FlsGetValue2` ;
- contrôle, SACL et contrôle RM des descripteurs de sécurité, `MakeSelfRelativeSD`
  et `MakeAbsoluteSD` ;
- notifications de veille, contextes de DPI et `SetWinEventHook` ;
- shcore (`GetDpiForMonitor`, `GetScaleFactorForMonitor`,
  `Set/GetProcessDpiAwareness`, contrat `api-ms-win-shcore-scaling-*`) ;
- les contrats `api-ms-win-downlevel-*` ;
- les bibliothèques de types d'oleaut32, par nom et par ordinal (#161 à #164,
  #183), qui rendent l'erreur documentée ;
- `CONOUT$`/`CONIN$` sans terminal, qui rendent la sortie ou l'entrée standard.

Test : `scripts/test-chromium-api.sh`.

### 11. Périphériques et entrées des jeux

`DeviceIoControl` (le plus demandé du corpus), entrée brute
(`RegisterRawInputDevices`, `GetRawInputData` : souris des jeux de tir), HID et
`setupapi`/`cfgmgr32` (`CM_Get_Parent`…, manettes et périphériques USB), ports
série (`SetCommState`…, 4 logiciels), `SetDeviceGammaRamp`, changement de
résolution (`ChangeDisplaySettingsExW`, 8).

Fait : `hid.dll` et l'énumération HID de `setupapi` sur hidraw, `DeviceIoControl`
sur disques, volumes et fichiers, entrée brute, ports série, définition
d'écran et rampe gamma, `cfgmgr32` et la classe Ports de `setupapi` (voir plus haut).
Reste : les transferts isochrones de `winusb`.

Ampleur : **M**. Suivi : #271.

### 12. Anciens DirectX et multimédia

`d3dx8`/`d3dx9_*`, DirectPlay, DirectShow (aujourd'hui `quartz` ne fait que
renvoyer des erreurs), la chaîne Media Foundation de lecture (#23), l'entrée
MIDI, `msacm32`. Ce sont les jeux et lecteurs des années 2000–2015, presque
toujours en 32 bits (point 3).

Fait (#237) : `ddraw.dll` maison (DirectDraw 1, 2, 4 et 7 : surfaces 8, 16,
24 et 32 bits, palettes, clés de couleur, `Blt`/`BltFast`, chaînes de `Flip`
calées sur 60 Hz, `GetDC`, découpeur, vrai changement de définition en plein
écran) ; `d3d8.dll` traduit Direct3D 8 sur Direct3D 9 (appareil complet,
textures, tampons, blocs d'états, déclarations et shaders vs.1.1 / ps.1.x) ;
`midiOut*` et `midiStream*` jouent sur un synthétiseur General MIDI maison
(128 instruments par familles, percussions sur le canal 10, 64 voix). Ces
objets lisent eux-mêmes la disposition x86 de leurs structures : ils servent
les jeux 32 bits comme les 64 bits. Test : `scripts/test-wow32.sh`
(`jeux32.exe` et `jeux64.exe`).

Ampleur : **L**. Suivi : #237, et #23 pour la vidéo.

### 13. Installeurs restants

NSIS, Inno Setup et MSI marchent ; il reste les plugins NSIS
(`System::Call`), les actions personnalisées MSI en DLL/EXE, WiX Burn,
InstallShield et les installeurs 32 bits (point 3).

Ampleur : **M** à **L**. Suivi : #179.

## Installateurs Squirrel (Claude pour Windows, Discord, Slack…)

`winlux --setup Setup.exe` reconnaît un installateur Squirrel.Windows : le
paquet zip est lu dans la ressource DATA 131 du Setup.exe 32 bits (qui n'est
pas exécuté), puis le .nupkg est déplié dans `%LocalAppData%\<Id>\app-<version>`,
avec Update.exe à la racine, un raccourci dans le menu Démarrer et un sur le
Bureau, et une entrée dans Applications installées. Essai réel : Claude 0.14.10
(Anthropic PBC) s'installe à l'identique, 324 Mo. Les imports directs de
claude.exe sont tous servis, sauf deux fonctions d'impression (winspool) en
bouchon. Le lancement d'Electron reste à vérifier en VM. Test :
`scripts/test-squirrel.sh`. ChatGPT pour Windows n'existe que dans le
Microsoft Store (paquet MSIX), que Winlux sait déjà installer.

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
