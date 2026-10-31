# Paint.NET sur Winlux

État au 31/10/2026, mesuré avec Paint.NET 5.1.12 portable x64.

## Résumé

**Paint.NET ne fonctionne pas encore.** Son lanceur natif
(`paintdotnet.exe`) traverse `hostfxr.dll` et `hostpolicy.dll`, puis démarre
le runtime que l'appli embarque (`coreclr.dll`). Depuis la PR 2, coreclr
s'initialise, charge `System.Private.CoreLib.dll`, exécute du code managé et
charge l'assembly principale `paintdotnet.dll`. Il s'arrête ensuite : une
exception C++ interne à coreclr est relancée en boucle, coreclr affiche
« Stack overflow. » puis lève une exception managée non gérée (HRESULT
0x8007007E, module introuvable).

## Obtenir l'appli

- La version portable suffit, sans installeur ni MSI. C'est aussi celle
  qu'utilise le catalogue Scoop Extras du Store (`bucket/paint.net.json`) :
  `paint.net.5.1.12.portable.x64.zip`, SHA-256
  `d5ae7043f2fb9d365b48dfe243a2aca1c74924de99b04b6445916c95354aefa3`.
- Une fois décompressé, le dossier pèse 307 Mo et contient 415 fichiers.
- Pour essayer : `winlux paintdotnet.exe` depuis le dossier décompressé.

## Ce que l'appli exige

| Élément | Détail |
|---|---|
| Runtime | .NET 9.0.13, **autonome** : `coreclr.dll`, `clrjit.dll`, `hostfxr.dll`, `hostpolicy.dll` et toute la BCL sont livrés dans le dossier |
| Code managé | 268 assemblies, 45 000 types, 405 000 méthodes |
| Interface | WPF (`PresentationFramework`, `PresentationCore`, natifs `wpfgfx_cor3.dll`, `PresentationNative_cor3.dll`, `DirectWriteForwarder.dll` en C++/CLI) et WinForms |
| Graphisme | Direct2D, DirectWrite, WIC, Direct3D 11, DXGI, DirectComposition, via TerraFX (`d2d1`, `dwrite`, `windowscodecs`, `d3d11`, `dxgi`, `dcomp`, `d3dcompiler_47`) |
| Autres API | `user32` (92 appels P/Invoke), `kernel32` (70), `gdi32`, `gdiplus`, `mscms` (gestion des couleurs), `uxtheme`, `dwmapi`, `propsys`, `shell32` |
| Natif propre | `PaintDotNet.SystemLayer.Native.x64.dll` (découpe de polygones, conversions float/half AVX2) |
| CRT | Visual C++ 2015+ (CRT universelle `api-ms-win-crt-*`, `vcruntime140`, `msvcp140`) |

## Choix d'exécution

Pour une appli .NET **autonome**, qui livre son propre `coreclr.dll` à côté
de son lanceur, Winlux exécute le lanceur natif. Celui-ci charge ce runtime
comme sous Windows, et rien n'est téléchargé en dehors de l'appli. Le CLR
intégré de Winlux (`runtime/src/clr`) reste utilisé pour les assemblies et
les applis qui n'embarquent pas de runtime (choix de Damien, 31/10/2026).

Le CLR intégré ne pouvait pas porter Paint.NET pour deux raisons :

- WPF repose sur des DLL natives et sur un composant C++/CLI mixte
  (`DirectWriteForwarder.dll`). Ce composant appelle du code managé depuis
  du code natif, ce qu'un interpréteur ne fournit pas.
- Sans JIT, les filtres d'image tourneraient en IL interprété, donc beaucoup
  trop lentement.

Essai avec le CLR intégré, avant ce choix : l'exécution s'arrête dès le
premier constructeur statique (`ImagingFactoryWrapper`). Ce CLR ne charge
qu'une assembly et ne gère ni les génériques, ni les types valeur, ni les
pointeurs de fonction.

## Ce qui marche maintenant

- L'aiguillage : un apphost dont le dossier contient `coreclr.dll` et
  `hostfxr.dll` part sur le chemin natif (`managed::bundles_own_runtime`).
- `paintdotnet.exe` lit sa ligne de commande, résout son dossier et charge
  `hostfxr.dll`.
- `hostfxr.dll` lit `paintdotnet.runtimeconfig.json` et
  `paintdotnet.deps.json`, puis charge `hostpolicy.dll`.
- `hostpolicy.dll` construit la liste des assemblies et charge
  `coreclr.dll`, dont les rappels TLS s'exécutent.

Fonctions ajoutées pour y arriver :

- La CRT universelle : la famille `__stdio_common_*` (printf et scanf de
  la CRT universelle), les chaînes sûres `*_s`, les variantes larges
  (`_wcsicmp`, `towlower`, `wcstoul`…), `ungetc` et
  `_get_stream_buffer_pointers` (pour les flux de la STL MSVC), et les
  fonctions mathématiques en simple précision.
- `kernel32` : `GetCurrentPackageId` (sans paquet MSIX), `IsWow64Process`,
  `LCMapStringEx`, `GetFinalPathNameByHandleW` et `FindFirstFileExW`.
- `advapi32` : `RegGetValueW`.
- `oleaut32` : les ordinaux publics (BSTR, VARIANT, SAFEARRAY).
- `shlwapi` : `PathRemoveFileSpecW` et `PathRemoveExtensionW`.

Ajouté par la PR 2 (démarrage de coreclr) :

- La répartition SEH x64 (`runtime/src/dlls/seh.rs`) : `RaiseException`,
  `RtlCaptureContext`, `RtlLookupFunctionEntry`, `RtlVirtualUnwind`,
  `RtlUnwindEx` avec consolidation, `RtlRestoreContext`, gestionnaires
  vectorisés et tables de fonctions dynamiques
  (`RtlAddFunctionTable`, `RtlInstallFunctionTableCallback`) pour le code du
  JIT. Les exceptions C++ MSVC de coreclr remontent jusqu'à leur `catch`.
- `GlobalMemoryStatusEx` (lu dans `/proc/meminfo`, nécessaire au tas du
  GC), `GetProcessGroupAffinity`, `FlushProcessWriteBuffers`,
  `WaitForMultipleObjectsEx`, `OpenEventW`, `OpenThreadToken`.
- `CreateThread` accepte `STACK_SIZE_PARAM_IS_A_RESERVATION` : sans lui, le
  thread du finaliseur ne démarrait pas et coreclr attendait sans fin.
- `VirtualQuery` décrit aussi la pile du thread et les projections Linux :
  coreclr y lit les bornes de sa pile.
- Les DLL relogées sont placées sur 64 Kio, comme sous Windows : coreclr
  refusait sinon l'en-tête de `System.Private.CoreLib.dll`.
- Le déroulage lit correctement `UWOP_SAVE_XMM128` (code 8) ; il le prenait
  pour `UWOP_PUSH_MACHFRAME` et plantait dans les stubs de coreclr.
- `VerSetConditionMask` et `VerifyVersionInfoW` (coreclr refusait « Windows
  7 minimum »), `InitializeContext2`, `CopyContext`, `LocateXStateFeature`,
  `IsWow64Process2`, `SetThreadDescription`, `GetTempPath2W`,
  `GetLocaleInfoEx`, `GetUserDefaultLocaleName`, `LocaleNameToLCID`,
  `GetUserPreferredUILanguages`, `EventSetInformation`,
  `RtlDllShutdownInProgress`.
- Diagnostic : `WINLUX_APITRACE=1` journalise chaque appel d'API intégrée
  avec son appelant, et `WINLUX_LOG=trace` chaque exception levée.

## La boucle d'exceptions après `paintdotnet.dll` (état au 31/10/2026)

Ce que montrent les journaux de Winlux (`WINLUX_LOG=trace`,
`WINLUX_APITRACE=1`) :

- coreclr charge `System.Private.CoreLib.dll` puis `paintdotnet.dll`
  (relogées, alignées sur 64 Kio). Le code managé s'exécute.
- Ensuite, la même exception C++ MSVC (0xE06D7363) revient une dizaine de
  fois. À chaque tour, il y a une levée avec objet, une relance `throw;`
  (arguments nuls) et une nouvelle levée de l'exception d'origine.
- La boucle se termine par « Stack overflow. », suivi d'une exception
  managée non gérée (0xE0434352, HRESULT 0x8007007E).
- Le test MinGW `cpp_exceptions` (lancer, relancer, try imbriqués) passe
  avec le SEH de Winlux. Le modèle de GCC n'exécute toutefois pas les blocs
  `catch` comme MSVC.

Hypothèse à vérifier : sous MSVC, un bloc `catch` s'exécute pendant le
rappel de consolidation de `RtlUnwindEx` (`RtlRestoreContext`). Une relance
`throw;` faite dans ce rappel doit être déroulée au-delà du cadre du `try`
déjà traité. Si notre substitution des cadres de trampoline
(`FrameKind::Consolidate` dans `seh.rs`) fait reprendre le parcours trop
bas, le même `catch` reprend l'exception à chaque fois, et la pile déborde.

Prochaine étape, avec nos propres outils : journaliser dans `seh.rs`
chaque cadre parcouru par `dispatch` (RIP, cadre d'établissement,
gestionnaire, disposition) et chaque entrée de `FRAMES`. Il faut aussi
un test de relance depuis un rappel de consolidation.

## Ce qui casse, dans l'ordre où on le rencontre

1. **coreclr, après le chargement de `paintdotnet.dll`** : une exception C++
   interne est relancée en boucle jusqu'au débordement de pile. Restent
   aussi à faire `QueueUserAPC2`, `SuspendThread`/`GetThreadContext`/
   `SetThreadContext` (suspension des threads par le GC), la conversion des
   signaux matériels (SIGSEGV) en exceptions SEH pour les
   `NullReferenceException`, et les imports de clrjit encore absents.
2. **BCL** : `System.Private.CoreLib` appelle directement 147 fonctions de
   kernel32, 34 de `System.Globalization.Native` (ICU) et quelques-unes de
   `ntdll`, `advapi32` et `ole32`.
3. **WIC** (`windowscodecs`) : tous les bitmaps de Paint.NET passent par
   WIC. Depuis la PR 3, Winlux fournit sa propre fabrique WIC
   (`runtime/src/dlls/windowscodecs.rs`), branchée sur les décodeurs maison :
   décodage (PNG, JPEG, GIF, BMP, TIFF, WebP, DDS) depuis un fichier, un flux
   ou un descripteur, bitmaps en mémoire avec `Lock`, conversion entre les
   formats 8, 24 et 32 bits (BGRA, RGBA, prémultipliés), mise à l'échelle,
   découpe, rotation et miroir, et `IWICStream`. Manquent encore les
   encodeurs (enregistrement), les palettes, les profils de couleur et les
   métadonnées. Comme Paint.NET s'arrête avant d'appeler WIC, cette partie
   n'est vérifiée que par les tests unitaires pour l'instant.
4. **Direct2D et DirectWrite** : Paint.NET peint le canevas et les contrôles
   en Direct2D, sur Direct3D 11 ou sur le rendu logiciel WARP. Depuis la
   PR 4, Winlux fournit un Direct2D 1.0 logiciel maison
   (`runtime/src/dlls/d2d1.rs`, sur le rasteriseur `d2d_raster.rs`) :
   fabrique, géométries rectangle, rectangle arrondi, ellipse et chemin
   (lignes, Bézier cubiques et quadratiques, arcs, règles pair-impair et
   non nulle, test de contenance et boîte englobante), pinceaux unis avec
   opacité, bitmaps (copie depuis la mémoire ou un bitmap WIC), cible de
   rendu sur bitmap WIC (`CreateWicBitmapRenderTarget`) avec effacement,
   remplissage et contour anticrénelés, `DrawBitmap` (plus proche voisin ou
   bilinéaire), transformations, découpe alignée sur les axes, et les
   fonctions `D2D1MakeRotateMatrix`, `D2D1MakeSkewMatrix`,
   `D2D1InvertMatrix`. Manquent encore : `ID2D1Factory1` et le contexte de
   périphérique (`ID2D1DeviceContext`, effets), les dégradés, les pinceaux
   bitmap, les styles de trait, les calques, les cibles HWND et DXGI, et
   tout DirectWrite (le texte n'est pas encore dessiné). Comme pour WIC, ce
   n'est vérifié que par les tests unitaires tant que Paint.NET s'arrête
   avant.
5. **WPF** : `wpfgfx_cor3.dll` (composition MIL, rendu Direct3D 9 ou
   logiciel), `PresentationNative_cor3.dll` et `DirectWriteForwarder.dll`
   (mixte, via `ijwhost.dll`).
6. **WinForms et GDI+** pour les boîtes de dialogue historiques.

## Ordre des PRs

1. Lanceur .NET autonome jusqu'au chargement de coreclr (#133).
2. coreclr : répartition SEH x64 et exceptions C++ MSVC, VEH, tables de
   fonctions dynamiques et imports de démarrage *(cette PR : coreclr exécute
   du code managé)*. Suite : suspension de threads, exceptions matérielles,
   imports de clrjit, jusqu'à un « Hello world » .NET 9 autonome.
3. BCL : imports directs de CoreLib et globalisation.
4. WIC : décodeurs branchés sur les décodeurs maison *(PR 3)*, encodeurs ensuite.
5. Direct2D 1.0 logiciel *(PR 4)*, puis `ID2D1DeviceContext` et DirectWrite.
6. WPF (`wpfgfx`) et la fenêtre principale de Paint.NET.

Chaque étape se valide sur un petit programme .NET autonome de référence
avant de passer à Paint.NET.
