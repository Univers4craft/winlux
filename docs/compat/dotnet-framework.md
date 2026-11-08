# Moteur .NET Framework maison : inventaire mesuré (8 novembre 2026)

Mesure faite sur master e3572c7, binaire `winlux` reconstruit, avec l'outil
`scripts/clr-inventaire.py` (lecture ECMA-335 des tables de métadonnées et des corps IL).

## Témoins

| Témoin | Cible | Assemblies référencées | Types externes | Membres externes | Membres sur génériques | P/Invoke |
|---|---|---|---|---|---|---|
| SetupDownloader.exe (installeur web Paint.NET 5.1.12) | .NET 4.7.2, WinForms, async | mscorlib, System, System.Windows.Forms, System.Drawing, System.Core, System.IO.Compression, Newtonsoft.Json | 129 | 278 | 92 | 15 |
| Newtonsoft.Json.dll 13 (livrée avec lui) | .NET 4.x, bibliothèque | mscorlib, System, System.Core, System.Xml, System.Xml.Linq, System.Numerics, System.Data, System.Runtime.Serialization | 228 | 841 | 1128 | 0 |
| KeePass 2.57.1 (GPL, portable) | .NET 2.0–4.x, WinForms | mscorlib, System, System.Windows.Forms, System.Drawing, System.Xml, System.Security | 472 | 2172 | 1895 | 183 |

Exécution réelle : SetupDownloader s'arrête à la toute première instruction
utile, `MissingMethodException System.OperatingSystem::get_Version`.

## Ce que fait le moteur actuel (≈ 5 980 lignes, runtime/src/clr)

- Une seule assembly chargée : aucune résolution d'AssemblyRef, donc
  Newtonsoft.Json ou toute DLL de l'appli est invisible.
- La bibliothèque de base est une liste de fonctions Rust appelées **par nom**
  (`System.Console::WriteLine`…) : pas de vrais types de base, donc une classe
  de l'appli ne peut pas dériver de `Form`, `Control`, `List<T>`, `Exception`
  de façon fidèle (tables virtuelles, champs, interfaces).
- Génériques : une instanciation est ramenée au nom du type ouvert, sans
  substitution de paramètres.
- Opcodes absents (sur 219) : `ldftn`, `ldvirtftn` (délégués), `ldelema`,
  `ldobj`, `stobj`, `ldelem`/`stelem` génériques, `ldelem.r4/r8/i`,
  `stelem.i/i2/r4/r8`, `calli`, `cpobj`, `endfilter`, `cpblk`, `initblk`,
  `jmp`, `mkrefany`, `refanyval`, `refanytype`, `arglist`, `ckfinite`.
  KeePass utilise `ldftn` 1 296 fois, `ldelema` 137, `stobj` 130.
- Pas de tests automatiques pour le CLR.

## Besoins mesurés, par brique

1. Chargeur multi-assemblies (AssemblyRef → dossier de l'appli, puis nos
   assemblies de base) : 7 à 8 références par témoin.
2. Système de types réel : tables de méthodes, interfaces, types valeur de
   taille quelconque, champs statiques par type, constructeurs statiques à la
   demande.
3. Génériques : 53 à 662 TypeSpec, 48 à 128 MethodSpec par témoin ;
   `constrained.` jusqu'à 401 fois.
4. Délégués et événements : `ldftn` partout (lambdas, `EventHandler`).
5. Exceptions : 28 méthodes avec clauses chez SetupDownloader, 1 294 chez
   KeePass (finally, filtres, rethrow).
6. Threads et async : SetupDownloader utilise `Task`, `async/await`,
   `CancellationTokenSource`, `ISynchronizeInvoke` ; le ramasse-miettes doit
   alors arrêter les threads (recoupe #269).
7. P/Invoke : 15 (SetupDownloader) à 183 (KeePass) vers kernel32, user32,
   advapi32, winhttp, wintrust, gdi32… et vers la DLL native de l'appli
   (KeePassLibC64).
8. Bibliothèque de base : mscorlib, System, System.Core, System.Drawing,
   System.Windows.Forms en premier ; System.Xml, System.IO.Compression,
   System.Numerics, System.Data ensuite.

## Étapes

| # | Étape | Témoin | État |
|---|---|---|---|
| 1 | Chargeur multi-assemblies, appels, héritage, champs et constructeurs statiques entre assemblies | `clr::tests` (Outils.dll + Programme) ; SetupDownloader charge Newtonsoft.Json | fait (#926) |
| 2 | Nouveau cœur d'exécution : disposition mémoire réelle, `ref`/`out`, structures copiées, génériques (TypeSpec, MethodSpec, `constrained.`), arithmétique ECMA | `clr::tests` (ref/out, structures, `Boite<T>`, arithmétique) | fait (#945, #946) |
| 3 | Délégués, événements, lambdas (`ldftn`, `ldvirtftn`, `Invoke`, multicast, `Delegate.Combine`/`Remove`, `Interlocked`) | `clr::tests` (délégué statique et d'instance, `Func<int>` par `ldvirtftn`, accesseur d'événement) | fait (#971) ; `BeginInvoke`/`EndInvoke` à l'étape 5 |
| 4 | Exceptions complètes : filtres `when`, finally imbriqués (pile de reprises par gestionnaire), `rethrow` du catch englobant, `InnerException` | `clr::tests` (filtres, rethrow à travers deux finally, `leave` à travers deux finally, finally d'un appelé) | fait (#972) ; filtres évalués en une seule passe |
| 5 | Fils managés : `Thread` (Start, Join, Sleep, IsBackground, CurrentThread), `lock`/`Monitor` (Wait, Pulse, TryEnter), `ThreadPool.QueueUserWorkItem`, `BeginInvoke`/`EndInvoke`. Un interpréteur par fil, verrou global rendu aux points sûrs, pendant les attentes et les appels natifs ; le processus attend ses fils de premier plan | `clr::tests` (quatre fils sous `lock`, `Wait`/`Pulse` avec ThreadPool, `BeginInvoke`) | fait (#1066) ; `Task` et `async/await` viennent avec la BCL C# (étape 7), `[ThreadStatic]` et `WaitHandle` à faire |
| 6 | P/Invoke complet : appel Win64 générique (flottants dans XMM, arguments sur la pile sans limite), structures disposées comme en C (`StructLayout`, `Pack`, `Size`, `FieldOffset`) par adresse ou par valeur, rappels natifs vers des délégués (64 portes), `SetLastError` et `Marshal.GetLastWin32Error`, TEB créé au lancement | `clr::tests` (`pow`, `sprintf` à six arguments, `PtInRect`, `SetLastError`, `qsort` rappelant un délégué) | fait (#994) ; DLL natives livrées avec l'appli et `StringBuilder` en sortie à faire |
| 7 | mscorlib, System, System.Core | `Update.exe --checkForUpdate` de Squirrel (mise à jour de Claude pour Windows, #1190) | à faire ; en attendant, une méthode absente lève `MissingMethodException` (#1190) |
| 8 | System.Drawing et WinForms sur user32/gdi32/comctl32 | fenêtre de SetupDownloader | à faire |
| 9 | KeePass complet, System.Xml | KeePass 2.57.1 | à faire |

Depuis l'étape 1, les constructeurs statiques tournent au premier usage de
leur type (ECMA-335 §II.10.5.3) et non plus tous au démarrage.
SetupDownloader charge Newtonsoft.Json depuis son dossier, ne s'arrête plus sur
`OperatingSystem::get_Version` et va jusqu'à `MessageBox::Show`, qui relève de
l'étape 8.

## Cœur d'exécution (depuis l'étape 2)

- Arguments et variables locales vivent dans une zone propre à chaque appel,
  qui ne bouge pas : `ldloca`/`ldarga` donnent de vraies adresses.
- Chaque valeur est rangée selon son type (`Kind`) : entiers sur leur largeur,
  références sur 8 octets, types valeur en place. Champs sur des emplacements
  multiples de 8 octets (le ramasse-miettes parcourt la mémoire par mots).
- Sur la pile d'évaluation, un type valeur est une copie temporaire dans le
  tas, recopiée à chaque rangement.
- Génériques : code IL partagé, contexte (`!0`, `!!0`) propre à chaque appel ;
  chaque instanciation est un type avec sa disposition et ses statiques.
- Tableaux : éléments à leur taille naturelle (`byte[]` = 1 octet par élément).
