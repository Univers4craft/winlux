# Compatibilité des applications

Cette page dit ce que le runtime Winlux sait faire pour un programme Windows, ce qu'il ne sait pas encore faire, et comment diagnostiquer un échec. La référence API par API est [COMPATIBILITY.md](https://github.com/Univers4craft/winlux/blob/master/COMPATIBILITY.md).

> **Rappel important.** Aucune couche tierce n'est utilisée : ni Wine, ni Proton, ni DXVK. Tout ce qui est décrit ici est écrit dans `runtime/`. Voir [But et principes](But-et-principes).

## En résumé

État au 22 novembre 2026 (dernière version publiée : v0.2.21 ; la liste des logiciels vérifiés un par un est sur le [site](https://univers4craft.github.io/winlux/fr/software.html)).

| Type de programme | État |
|---|---|
| Programme console 64 ou 32 bits (C, C++, Rust, Go…) | **Fonctionne en général**, si ses imports sont implémentés. |
| Programme graphique Win32/GDI simple, 64 ou 32 bits | **Fonctionne souvent** : fenêtres, menus, dialogues communs, GDI, polices. |
| Installateur `.msi`, NSIS, Inno Setup, Burn, auto-extractible 7-Zip | **Fonctionne** : le contrat de licence est toujours montré avant d'installer, un refus n'installe rien, les actions personnalisées EXE et DLL s'exécutent. |
| Programme .NET Framework (sans runtime embarqué) | **Partiel** : notre moteur .NET couvre collections, LINQ, async/await, réflexion, fichiers, registre, processus, et une partie de WinForms (fenêtres, étiquettes, boutons, barres de progression, polices, icônes). |
| Application .NET autonome (livre son `coreclr.dll`) | **Démarre** son propre runtime ; Paint.NET s'ouvre et dessine, une application WinForms autonome simple s'ouvre. |
| WPF | **Partiel** : une application WPF autonome simple s'ouvre, mais le rendu est incomplet (#268). |
| Jeux Direct3D 9/11/12, OpenGL, Vulkan | **Partiel** : Warzone 2100, SuperTuxKart et Xonotic se jouent ; à valider jeu par jeu et sur de vrais GPU. |
| Applications Chromium, Electron, WebView2 | **En cours** (#236). |
| Paquets MSIX / AppX | **Installation, raccourcis et désinstallation** (#272). |
| Pilotes Windows (`.sys`), antivirus, anti-triche noyau | **Hors périmètre** : il n'y a pas de noyau Windows. Un anti-triche noyau est refusé à l'installation, avec un message clair. |

## Ce qui bloque encore les gros logiciels

La v0.3 vise que chaque logiciel testé s'installe et ouvre sa première fenêtre, jusqu'à l'écran de connexion quand il en a un. Les blocages actuels, chacun avec son issue :

1. **Office** : l'outil de déploiement télécharge Office et vérifie ses signatures, l'installation ne va pas encore au bout (#2740).
2. **Teams** : il démarre sans atteindre l'écran de connexion (#2511, #2726).
3. **Acrobat Reader** et les applications Chromium : #236.
4. **Creative Cloud** : le contrôle WebBrowser n'exécute pas encore les scripts de sa page (#2784).
5. **Programmes 32 bits** : la frontière entre le code x86 et nos DLL est la première source de défauts ; des interfaces COM rendues au code 32 bits n'ont pas encore de mandataire (#2428).
6. **Installeur web de Paint.NET** : son téléchargeur WinForms doit s'exécuter lui-même (#2752).

L'état des lieux mesuré, fonction par fonction et DLL par DLL, est dans [docs/COMPATIBILITE.md](https://github.com/Univers4craft/winlux/blob/master/docs/COMPATIBILITE.md). Pour mesurer un logiciel précis : `winlux --exports > exports.tsv`, puis `python3 scripts/compat-imports.py exports.tsv DOSSIER_DU_LOGICIEL`.

## Le chargeur

- Formats **PE32+ x86-64** et **PE32 x86** (les programmes 32 bits tournent dans le même processus, par la couche wow32, sans bibliothèques i386) : sections, relocations, imports par nom et par ordinal, imports différés, ressources, manifestes (élévation, DPI).
- **DLL réelles** livrées avec le programme : chargées avec `LoadLibraryA/W`, `GetProcAddress`, callbacks TLS et `DllMain`.
- **Contrats API-set** (`api-ms-win-*`) redirigés vers les modules correspondants.
- **Exceptions** : SEH x64 (`RtlUnwindEx`, `RtlVirtualUnwind`, `RtlLookupFunctionEntry`), exceptions C++ MSVC (`_CxxThrowException`, `__CxxFrameHandler3`, `__C_specific_handler` pour `__try/__except/__finally`), `__CxxFrameHandler4`, exceptions MinGW via `libgcc`/`libstdc++`.
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
- **Direct2D** (logiciel, maison) : géométries, chemins, pinceaux unis et dégradés, bitmaps, anticrénelage, transformations, découpe, contextes de périphérique (`ID2D1DeviceContext`) utilisés par Paint.NET.
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
- **Limite** : Media Foundation et DirectShow ne décodent pas encore toute la vidéo pour les `.exe` (le lecteur multimédia intégré, lui, s'appuie sur FFmpeg et lit tous les formats courants).

## Réseau

Winsock (TCP, UDP, IPv6, multicast, `getaddrinfo`), WinHTTP et `iphlpapi`. Les ports ouverts par un `.exe` apparaissent dans le pare-feu avec le nom de l'application, et le proxy système est partagé entre applications Windows et Linux. Voir [Réseau et disques](Reseau-et-disques).

## .NET

Winlux a deux chemins d'exécution pour le code managé :

1. **Le moteur .NET maison** (`runtime/src/clr/`), pour les assemblies et les applications qui n'embarquent pas leur runtime (.NET Framework) : lecture des métadonnées, interpréteur CIL, ramasse-miettes, exceptions .NET, génériques, délégués, LINQ, async/await, réflexion et P/Invoke vers les DLL intégrées. Sa bibliothèque de base (`mscorlib` maison) couvre les collections, les fichiers, le registre, les processus, `System.Net` sur notre WinHTTP, `System.IO.Compression`, et une partie de WinForms et de `System.Drawing`. Il accepte les assemblies 32 et 64 bits. **Limites** : WinForms reste partiel et WPF absent de ce chemin ; une méthode qui manque lève `MissingMethodException`, et `WINLUX_CLR_MANQUANTS=<fichier>` en dresse la liste avec l'appelant.
2. **Le runtime embarqué par l'application** : une application .NET autonome qui livre son `coreclr.dll` à côté de son lanceur (comme Paint.NET) est lancée par son lanceur natif, qui charge ce runtime lui-même. Rien n'est téléchargé.

Aucun runtime .NET Linux (`dotnet`, Mono) n'est nécessaire.

### Le cas Paint.NET

Paint.NET 5.1 est l'application de référence pour .NET de bureau (issue #138, [docs/PAINTNET.md](https://github.com/Univers4craft/winlux/blob/master/docs/PAINTNET.md)). Depuis la v0.2.21, il s'installe, s'ouvre et dessine avec le runtime qu'il embarque, Direct2D, DirectWrite et WIC maison. Restent des défauts d'affichage, comme la vignette de l'onglet d'image (#2557), et son installeur web, dont le téléchargeur WinForms passe par notre moteur .NET maison (#2752).

## Installer des logiciels Windows

- **`.msi`** : `msiexec` intégré (`/i`, `/x`, `/qn`, propriétés, CAB avec LZX…), actions personnalisées EXE et DLL, actions WiX ([docs/MSIEXEC.md](https://github.com/Univers4craft/winlux/blob/master/docs/MSIEXEC.md)).
- **NSIS 3, Inno Setup, Burn, auto-extractibles 7-Zip** : l'assistant de l'installeur s'affiche, avec son contrat de licence ; raccourcis, clé `Uninstall`, désinstallation ([docs/SETUP-EXE.md](https://github.com/Univers4craft/winlux/blob/master/docs/SETUP-EXE.md)).
- **Signatures** : la signature Authenticode d'un programme, d'un paquet MSIX ou d'un cabinet est vérifiée, horodatage compris ; l'invite d'élévation nomme l'éditeur lu dans la signature.
- **Store** : la source « Windows » s'appuie sur le catalogue Scoop Extras (installateurs officiels des éditeurs, sommes de contrôle vérifiées).
- **Applications installées** (`appwiz.cpl`) liste les programmes et les désinstalle avec le bon outil.

## Applications testées

La liste à jour des logiciels et des jeux vérifiés un par un au banc de test, avec leurs issues, est sur la page [Logiciels et jeux](https://univers4craft.github.io/winlux/fr/software.html) du site. En bref : VLC, Audacity, ScreenToGif, OBS Studio, Paint.NET, 7-Zip, Notepad++, SumatraPDF, Flow Launcher, PuTTY, FileZilla, WinSCP, Claude Code et Pythonwin s'ouvrent ; Office, Teams, Acrobat, Creative Cloud et Steam sont en cours.

## Diagnostiquer un programme qui ne marche pas

```sh
WINLUX_LOG=debug winlux programme.exe        # journal détaillé
WINLUX_TRACE=1 winlux programme.exe          # trace du chargeur
winlux --inspect programme.exe               # en-têtes, imports, dépendances
```

1. Si le programme s'arrête sur **« import manquant »**, le nom de la DLL et de la fonction sont dans le message et dans l'Observateur d'événements.
2. Après un plantage, Winlux propose **« Signaler un problème »**, qui prépare une issue GitHub avec la cause probable, la version et le journal, données personnelles masquées.

## Ce qui reste à faire

- Les blocages des gros logiciels listés plus haut (Office, Teams, Acrobat, Creative Cloud, installeur web de Paint.NET).
- Chromium, Electron et WebView2 (#236) ; WPF complet (#268) ; un vrai runtime .NET Framework pour WinForms et WPF (#22).
- Mandataires COM pour les programmes 32 bits (#2428) ; serveurs COM hors processus déclarés comme service (#2575).
- Les fonctions encore absentes que chaque bench signale, une issue par fonction ou par famille.

Voir aussi : [Limites et problèmes connus](Limites-et-problemes-connus) · [Feuille de route](Feuille-de-route)
