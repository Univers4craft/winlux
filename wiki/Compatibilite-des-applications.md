# Compatibilité des applications

Cette page dit ce que le runtime Winlux sait faire pour un programme Windows, ce qu'il ne sait pas encore faire, et comment diagnostiquer un échec. La référence API par API est [COMPATIBILITY.md](https://github.com/Univers4craft/winlux/blob/master/COMPATIBILITY.md).

> **Rappel important.** Aucune couche tierce n'est utilisée : ni Wine, ni Proton, ni DXVK. Tout ce qui est décrit ici est écrit dans `runtime/`. Voir [But et principes](But-et-principes).

## En résumé

| Type de programme | État |
|---|---|
| Programme console 64 bits (C, C++, Rust, Go…) | **Fonctionne en général**, si ses imports sont implémentés. |
| Programme graphique Win32/GDI 64 bits simple | **Fonctionne souvent** : fenêtres, menus, dialogues communs, GDI, polices. |
| Installateur `.msi` | **Fonctionne** avec le `msiexec` intégré (sans actions personnalisées DLL/EXE/script). |
| Installateur NSIS ou Inno Setup | **Fonctionne** : testé avec Notepad++, WinMerge, ShareX, Audacity (installation en 1 à 4 secondes). |
| Programme .NET simple (console, BCL de base) | **Fonctionne** avec le CLR maison, dans ses limites (voir plus bas). |
| Application .NET autonome (livre son `coreclr.dll`) | **Démarre** son propre runtime ; Paint.NET va jusqu'au code managé mais ne s'ouvre pas encore. |
| WinForms, WPF | **Pas encore.** |
| Jeux Direct3D 9/11/12 | **Partiel** : traduction maison vers OpenGL/Vulkan, à valider sur vrais GPU et vrais jeux. |
| Programme 32 bits (PE32) | **Non pris en charge** : refusé avec un message clair. |
| Pilotes Windows (`.sys`), antivirus, anti-triche noyau | **Hors périmètre** : il n'y a pas de noyau Windows. |
| Paquets MSIX / Store Microsoft | **Non pris en charge.** |

## Ce qui reste pour presque tout faire tourner

L'état des lieux mesuré (21 vrais logiciels, 1 834 fonctions et 73 DLL qui manquent, classés par nombre d'applications bloquées) est dans [docs/COMPATIBILITE.md](https://github.com/Univers4craft/winlux/blob/master/docs/COMPATIBILITE.md). En résumé, par ordre d'impact :

1. Les fonctions Win32 de base les plus demandées et les petites DLL absentes (`imm32`, `psapi`, `msimg32`…) : #14, #26.
2. OpenGL et Vulkan pour les applis : #233.
3. Le dossier System32 et les redistribuables Visual C++ : #234.
4. HTTPS et sécurité (Schannel, certificats, WinINet) : #235.
5. Les shaders Direct3D 10/11/12 : #137.
6. Les programmes 32 bits : #232.
7. Chromium, Electron et WebView2 : #236.
8. WinForms et WPF : #22, #138.
9. Les anciens DirectX et le MIDI : #237.

Pour mesurer un logiciel précis : `winlux --exports > exports.tsv`, puis `python3 scripts/compat-imports.py exports.tsv DOSSIER_DU_LOGICIEL`.

## Le chargeur

- Format **PE32+ x86-64** : sections, relocations, imports par nom et par ordinal, imports différés, ressources, manifestes (élévation, DPI).
- **DLL réelles** livrées avec le programme : chargées avec `LoadLibraryA/W`, `GetProcAddress`, callbacks TLS et `DllMain`.
- **Contrats API-set** (`api-ms-win-*`) redirigés vers les modules correspondants.
- **Exceptions** : SEH x64 (`RtlUnwindEx`, `RtlVirtualUnwind`, `RtlLookupFunctionEntry`), exceptions C++ MSVC (`_CxxThrowException`, `__CxxFrameHandler3`, `__C_specific_handler` pour `__try/__except/__finally`), exceptions MinGW via `libgcc`/`libstdc++`. `__CxxFrameHandler4` est à venir.
- **Politique des imports manquants** : par défaut, un import non implémenté **arrête le programme** proprement, l'inscrit dans le journal et propose un signalement. `WINLUX_MISSING=ignore` permet de continuer pour diagnostiquer (avec un risque de résultats faux).

## Système, fichiers et processus

- **Fichiers** : API A/W complètes pour l'usage courant (`CreateFile`, `CopyFile`, `MoveFile`, `FindFirstFile` avec jokers, attributs, liens symboliques, chemins longs), E/S *overlapped* et ports de complétion IOCP (sous-ensemble), `NtCreateFile`/`NtOpenFile`. Les chemins sont traduits : voir [Chemins et droits](Chemins-et-droits).
- **Mémoire et threads** : `VirtualAlloc/Protect/Query`, TLS et FLS, sections critiques, événements, mutex, sémaphores comptés, variables de condition, `WaitForMultipleObjects`, pool de threads.
- **Processus** : `CreateProcess` lance un `.exe` (via Winlux) ou un programme Linux, avec attente, code de sortie, `CREATE_SUSPENDED`. `OpenProcess`/`TerminateProcess`/`SetPriorityClass` agissent sur les vrais processus.
- **Registre** : complet pour l'usage applicatif, partagé entre toutes les applications. Voir [Registre Windows](Registre-Windows).
- **Services** : un gestionnaire de services local enregistre les services dans le registre et lance leurs exécutables.
- **Sécurité** : jetons, SID et descripteurs de sécurité tirés des droits Linux ; UAC maison.
- **Langues et texte** : pages de code 1252, 850, 858, 437, ISO-8859, UTF-8 ; console en page OEM ; dispositions clavier XKB converties en HKL, touches mortes ; fuseaux horaires TZif et conversions FILETIME ; polices de `C:\Windows\Fonts` et remplacements d'Arial et Segoe UI ([docs/INTL.md](https://github.com/Univers4craft/winlux/blob/master/docs/INTL.md)).
- **Presse-papiers** et **glisser-déposer** communs avec les applications Linux (XDND).

## Interface graphique

- **user32** : classes, fenêtres, file de messages par thread, `GetMessage`/`DispatchMessage`, clavier et souris, focus, `SetWindowPos`, minuteurs, menus, dialogues, zone de notification (`Shell_NotifyIcon`).
- **Deux backends** : X11 (vraies fenêtres sur le serveur X, texte Xft, échelle DPI commune) et *headless* (sans affichage, pour les tests).
- **GDI** : pinceaux, plumes, polices, `BitBlt`/`StretchBlt`, DIB 1 à 32 bits, `CreateDIBSection`, rendu en double tampon. Les régions et certaines opérations raster restent à faire.
- **comctl32 / comdlg32** : contrôles communs et boîtes Ouvrir, Enregistrer, Imprimer.
- **Direct2D 1.0** (logiciel, maison) : géométries, chemins, pinceaux unis, bitmaps, anticrénelage, transformations, découpe. `ID2D1DeviceContext` et dégradés à venir.
- **DirectWrite** (maison) : lecture des contours TrueType sans dépendance, polices système et de remplacement, mise en page du texte. Mise en forme OpenType et polices CFF à venir.
- **WIC** : BMP, PNG, JPEG, GIF, TIFF, WebP… avec les décodeurs maison de `runtime/src/imaging/`.

## 3D, vidéo et jeux

- **Direct3D 11** traduit vers **OpenGL** ; **Direct3D 9 et 12** avec un backend **Vulkan** ; traduction de shaders maison (`d3d_shader`, `d3d9_shader`, `spirv`).
- **DXVA2** : décodage vidéo matériel H.264.
- **Manettes** XInput et DirectInput (evdev).
- **Mode jeu** : un `.exe` Direct3D/OpenGL/Vulkan/XInput active automatiquement `winlux --gamemode` (gouverneur performances sur secteur, priorité, sauvegardes et mises à jour en pause), tout est restauré à la fermeture.
- **Overlay** de performances activable dans Paramètres › Graphismes.
- À faire : shaders DXBC/DXIL complets, profondeur et alias de ressources en D3D12, autres codecs DXVA2, validation sur de vrais GPU (issue #137).

## Son, vidéo et périphériques

- **Son** : waveOut/waveIn, PlaySound, DirectSound, WASAPI (`mmdevapi`), XAudio2, vers **PipeWire** (volume par application) ou ALSA en repli ([docs/AUDIO.md](https://github.com/Univers4craft/winlux/blob/master/docs/AUDIO.md)).
- **Webcam** par Media Foundation (capture V4L2), **micro** par waveIn/DirectSound/WASAPI, soumis aux [autorisations](Chemins-et-droits).
- **Impression** : `winspool` et `comdlg32` maison ; les DC imprimante GDI sont rendus en PDF et envoyés à CUPS par IPP, ou enregistrés par l'imprimante virtuelle « Imprimer en PDF (Winlux) ».
- **Limite** : Media Foundation et DirectShow ne décodent pas encore la vidéo pour les `.exe` (le lecteur multimédia intégré, lui, s'appuie sur FFmpeg et lit tous les formats courants).

## Réseau

Winsock (TCP, UDP, IPv6, multicast, `getaddrinfo`), WinHTTP et `iphlpapi`. Les ports ouverts par un `.exe` apparaissent dans le pare-feu avec le nom de l'application, et le proxy système est partagé entre applications Windows et Linux. Voir [Réseau et disques](Reseau-et-disques).

## .NET

Winlux a deux chemins d'exécution pour le code managé :

1. **Le CLR maison** (`runtime/src/clr/`), pour les assemblies et les applications qui n'embarquent pas leur runtime : lecture des métadonnées, interpréteur CIL, ramasse-miettes, exceptions .NET, appels virtuels, BCL minimale (Console, Math, String, Environment, File, Path…) et P/Invoke vers les DLL intégrées. Il accepte les assemblies 32 et 64 bits. **Limites** : pas encore de génériques de la BCL (`List<T>`, `Dictionary`), de délégués ni de LINQ ; structures traitées comme des références ; paramètres flottants refusés en P/Invoke.
2. **Le runtime embarqué par l'application** : une application .NET autonome qui livre son `coreclr.dll` à côté de son lanceur (comme Paint.NET) est lancée par son lanceur natif, qui charge ce runtime comme sous Windows. Rien n'est téléchargé.

Aucun runtime .NET Linux (`dotnet`, Mono) n'est nécessaire.

### Le cas Paint.NET

Paint.NET 5.1.12 est l'application de référence pour .NET de bureau (issue #138, [docs/PAINTNET.md](https://github.com/Univers4craft/winlux/blob/master/docs/PAINTNET.md)). Aujourd'hui : son lanceur traverse `hostfxr` et `hostpolicy`, `coreclr` s'initialise, charge la bibliothèque de base et l'assembly principale, puis s'arrête sur une exception interne. Il reste notamment `__CxxFrameHandler4`, puis WPF. **Paint.NET ne s'ouvre donc pas encore.**

## Installer des logiciels Windows

- **`.msi`** : `msiexec` intégré (`/i`, `/x`, `/qn`, propriétés, CAB avec LZX…). Pas encore d'actions personnalisées, de `.mst`, de `.msp` ([docs/MSIEXEC.md](https://github.com/Univers4craft/winlux/blob/master/docs/MSIEXEC.md)).
- **NSIS 3 et Inno Setup** : contenu extrait et installé directement, raccourcis, clé `Uninstall`, désinstallation ([docs/SETUP-EXE.md](https://github.com/Univers4craft/winlux/blob/master/docs/SETUP-EXE.md)).
- **Store** : la source « Windows » s'appuie sur le catalogue Scoop Extras (installateurs officiels des éditeurs, sommes de contrôle vérifiées).
- **Applications installées** (`appwiz.cpl`) liste les programmes et les désinstalle avec le bon outil.

## Applications testées

| Application | Résultat |
|---|---|
| Notepad++ 8.6.9, WinMerge 2.16, ShareX 16.1, Audacity 3.6 | Installateurs NSIS/Inno : installés et désinstallés proprement. |
| Applications maison du shell (Explorateur, Gestionnaire des tâches, Regedit…) | Fonctionnent : ce sont des `.exe` exécutés par le runtime à chaque session. |
| Paint.NET 5.1.12 | Le runtime embarqué démarre, l'interface ne s'ouvre pas encore. |
| ChatGPT et Claude pour Windows | Ne démarrent pas encore ([docs/CLAUDE-CHATGPT-WINDOWS.md](https://github.com/Univers4craft/winlux/blob/master/docs/CLAUDE-CHATGPT-WINDOWS.md), [docs/CHATGPT-WINDOWS.md](https://github.com/Univers4craft/winlux/blob/master/docs/CHATGPT-WINDOWS.md)). |
| Photoshop | Non pris en charge. |

## Diagnostiquer un programme qui ne marche pas

```sh
WINLUX_LOG=debug winlux programme.exe        # journal détaillé
WINLUX_TRACE=1 winlux programme.exe          # trace du chargeur
winlux --inspect programme.exe               # en-têtes, imports, dépendances
```

1. Si le programme s'arrête sur **« import manquant »**, le nom de la DLL et de la fonction sont dans le message et dans l'Observateur d'événements.
2. **« programme Windows 32 bits »** : le binaire est en PE32, non pris en charge.
3. Après un plantage, Winlux propose **« Signaler un problème »**, qui prépare une issue GitHub avec la cause probable, la version et le journal, données personnelles masquées.

## Ce qui reste à faire

- WinForms puis WPF (et `__CxxFrameHandler4`) pour les applications .NET de bureau.
- Génériques, délégués et LINQ dans le CLR maison.
- COM : activation depuis le registre, serveurs hors processus, automatisation.
- Media Foundation/DirectShow branchés sur le moteur multimédia du runtime.
- Héritage des handles, objets nommés partagés entre processus, `FreeLibrary` réel, `StartServiceCtrlDispatcher`.
- Programmes 32 bits.

Voir aussi : [Limites et problèmes connus](Limites-et-problemes-connus) · [Feuille de route](Feuille-de-route)
