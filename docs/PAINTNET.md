# Paint.NET sur Winlux

État au 31/10/2026, mesuré avec Paint.NET 5.1.12 portable x64.

## Résumé

**Paint.NET ne fonctionne pas encore.** Avec la PR « lanceur .NET autonome »,
son lanceur natif (`paintdotnet.exe`) traverse `hostfxr.dll` et
`hostpolicy.dll`, puis charge le runtime que l'appli embarque
(`coreclr.dll`). L'initialisation de coreclr s'arrête ensuite sur une
exception C++ MSVC, car Winlux ne sait pas encore la dérouler.

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

## Ce qui casse, dans l'ordre où on le rencontre

1. **coreclr : exceptions C++ et SEH.** `GetProcessGroupAffinity` manque
   d'abord. Quand on l'ignore, coreclr lève une exception C++ (0xE06D7363)
   que Winlux ne sait pas propager : il manque la répartition SEH x64 vers
   `__CxxFrameHandler4`. Il manque aussi 136 imports de coreclr, dont
   `AddVectoredExceptionHandler`, `RtlAddFunctionTable` (code du JIT),
   `RtlRestoreContext`, `SuspendThread`/`GetThreadContext`/`SetThreadContext`
   (suspension des threads par le GC), `FlushProcessWriteBuffers`, les
   minuteurs de pool de threads et `CoGetObjectContext`. Il en manque 43
   dans clrjit, surtout des fonctions mathématiques.
2. **BCL** : `System.Private.CoreLib` appelle directement 147 fonctions de
   kernel32, 34 de `System.Globalization.Native` (ICU) et quelques-unes de
   `ntdll`, `advapi32` et `ole32`.
3. **WIC** (`windowscodecs`) : tous les bitmaps de Paint.NET passent par
   WIC.
4. **Direct2D et DirectWrite** : Paint.NET peint le canevas et les contrôles
   en Direct2D, sur Direct3D 11 ou sur le rendu logiciel WARP.
5. **WPF** : `wpfgfx_cor3.dll` (composition MIL, rendu Direct3D 9 ou
   logiciel), `PresentationNative_cor3.dll` et `DirectWriteForwarder.dll`
   (mixte, via `ijwhost.dll`).
6. **WinForms et GDI+** pour les boîtes de dialogue historiques.

## Ordre des PRs

1. Lanceur .NET autonome jusqu'au chargement de coreclr *(cette PR)*.
2. coreclr : répartition SEH x64 et exceptions C++ MSVC, VEH, tables de
   fonctions dynamiques, suspension de threads, et imports manquants de
   coreclr et clrjit. Objectif : faire tourner un « Hello world » .NET 9
   autonome.
3. BCL : imports directs de CoreLib et globalisation.
4. WIC : décodeurs et encodeurs branchés sur les décodeurs maison.
5. Direct2D et DirectWrite.
6. WPF (`wpfgfx`) et la fenêtre principale de Paint.NET.

Chaque étape se valide sur un petit programme .NET autonome de référence
avant de passer à Paint.NET.
