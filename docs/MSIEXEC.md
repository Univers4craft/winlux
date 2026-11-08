# msiexec intégré

Winlux installe les paquets Windows Installer (`.msi`) avec son propre
msiexec, écrit dans le runtime (`runtime/src/msi`) sans Wine ni bibliothèque
tierce. Il n'existe pas de fichier `msiexec.exe` : le runtime reconnaît ce nom.

```sh
winlux msiexec.exe /i paquet.msi /qn INSTALLDIR="C:\Mon Appli"
winlux --msiexec /x {CODE-PRODUIT}        # ou /x paquet.msi
winlux --msiexec /a paquet.msi TARGETDIR=C:\extrait
winlux --msiexec --list                    # paquets installés
winlux --msiexec --info paquet.msi         # code produit, nom, version
```

Un double-clic sur un `.msi` (Explorateur, bureau Linux) l'installe, le Store
s'en sert pour les catalogues Windows, et un programme Windows qui lance
`msiexec.exe` par `CreateProcess` obtient le même msiexec.

## Ce qui est pris en charge

| Domaine | Détail |
|---|---|
| Conteneur | fichier composé OLE (secteurs 512 o et 4 Ko, FAT, mini-FAT, DIFAT), noms de flux MSI compressés |
| Base | pool de chaînes (références 2 ou 3 octets, chaînes > 64 Ko, pages de code 1252/UTF-8), tables `_Columns`/`_Tables`, résumé (plateforme, fichiers compressés, noms courts) |
| Cabinets | intégrés (`#flux`) ou externes, non compressés, MSZIP (DEFLATE maison avec historique entre blocs), LZX (fenêtres 32 Ko à 2 Mo, blocs verbatim/aligné/non compressé, translation E8) |
| Logique | propriétés système et dossiers standard, propriétés de la ligne de commande, conditions (`AND`, `OR`, `NOT`, comparaisons, `~=`, `><`, `<<`, `>>`), chaînes formatées (`[PROP]`, `[#fichier]`, `[$composant]`, `[%ENV]`, `[~]`), actions personnalisées 51 et 35 (propriété, dossier) |
| Sélection | fonctionnalités par `INSTALLLEVEL`, table `Condition`, `ADDLOCAL`, `REMOVE`, conditions de composant |
| Actions | `File`, `DuplicateFile`, `CreateFolder`, `Registry` (chaînes, entiers, binaire, multi-chaînes, expand), `Shortcut` (y compris annoncés, en `.lnk`), inscription dans *Programmes installés* (`HKLM\…\Uninstall\{code}`) |
| Mises à jour | même `ProductCode` ou même `UpgradeCode` : l'ancienne version est retirée avant l'installation |
| Recherche | `AppSearch` par `RegLocator` (dossier, fichier ou valeur brute, vue 64 bits), `IniLocator`, `DrLocator` (profondeur limitée) |
| Environnement | table `Environment` : `HKCU\Environment` ou, avec `*`, celui du système ; création, `+` si absente, `!` retrait, ajout en tête ou en queue (`[~];valeur`). Les `.exe` lancés par Winlux reçoivent ces variables (sauf `PATH`) |
| Fichiers .ini | table `IniFile` : ajout, création si absente, ajout à une liste ; dans le dossier indiqué ou `C:\windows\` |
| Suppressions | `RemoveFile` (motifs `*`/`?`, dossiers vides, à l'installation et/ou à la désinstallation), `RemoveRegistry` |
| Actions EXE | actions personnalisées 2 (table `Binary`), 18 (fichier installé), 34 (dossier + ligne de commande), 50 (propriété) de `InstallExecuteSequence`, avec leur condition ; asynchrones (0x80), code ignoré (0x40), différées (0x400) exécutées à leur place, jamais celles de retour arrière ou de validation. Même politique que les installateurs NSIS/Inno (`setup/run.rs`) : un `.exe` passe par Winlux, jamais un programme Linux ni un outil système absent (`cmd`, `powershell`…) |
| Actions DLL de WiX | `WixQuietExec`, `CAQuietExec`, `WixSilentExec` (et 64 bits) reconnues sans charger la DLL : la ligne de commande rangée dans la propriété de l'action (ou son `CustomActionData` posé par une action 51) est lancée comme une action EXE ; `WixShellExec` (ouverture en fin d'interface) est journalisée |
| Actions DLL | types 1 (table `Binary`) et 17 (DLL installée), x86 et x64 : la DLL est chargée dans un processus à part (`winlux --msi-dll-action DLL ENTREE`) et son point d'entrée reçoit une session qui répond aux fonctions publiques de msi.dll (`MsiGetProperty`, `MsiSetProperty`, `MsiGetMode`, `MsiProcessMessage`, enregistrements, `MsiFormatRecord`, `MsiEvaluateCondition`, `MsiGetTargetPath`, états de composant et de fonctionnalité). Une action immédiate peut poser des propriétés pour la suite ; une action différée lit son `CustomActionData`. Une action immédiate lit la base du paquet en SQL (`MsiGetActiveDatabase`, `MsiDatabaseOpenView`, `MsiViewExecute`, `MsiViewFetch`, `MsiViewGetColumnInfo`, `MsiDatabaseIsTablePersistent` : `SELECT [DISTINCT] … FROM t1, t2 WHERE … ORDER BY …`, paramètres `?`), ; `MsiViewModify` (insertion, insertion temporaire, mise à jour, affectation, remplacement, fusion, suppression sur une vue d'une seule table) change la base en mémoire le temps de l'action, jamais le .msi, comme les changements temporaires d'une session Windows Installer ; les colonnes binaires (`Binary`, `Icon`…) se lisent par `MsiRecordReadStream` et `MsiRecordDataSize` ; une action différée n'a pas de base, comme sous Windows. `MsiDoAction` d'une action personnalisée la lance au retour de la DLL (motif « Sched… » de WiX : propriété posée puis action différée demandée). Un échec est journalisé, l'installation continue |
| Actions script | VBScript (types 6 : table `Binary`, 22 : fichier installé, 38 : texte dans `Target`, 54 : texte dans une propriété) et JScript (5, 21, 37, 53), immédiates ou différées, exécutés dans un processus à part (`winlux --msi-script-action LANGAGE FICHIER [FONCTION]`) par des interpréteurs maison écrits d'après les spécifications publiques (ECMAScript 3, référence du langage VBScript). Objets : `Session` (`Property`, `Mode`, `TargetPath`, `SourcePath`, `Language`, `EvaluateCondition`, `DoAction`, `Message`, `FormatRecord`, états, `Installer.CreateRecord`, `Database.OpenView` → `View.Execute/Fetch/Modify/Close`, `Record.StringData/IntegerData/ReadStream`), `WScript.Shell` (`RegRead`, `RegWrite`, `RegDelete`, `Run`, `ExpandEnvironmentStrings`, `Environment`), `Scripting.FileSystemObject` (fichiers, dossiers, `TextStream`), `Scripting.Dictionary`, `RegExp` / `/…/g` ; `MsgBox` est écrit dans le journal. Valeur rendue : 1 ou rien = succès, 2 = abandon (1602), 3 = échec (1603) ; une erreur de script (syntaxe ou exécution, avec son message) donne 1720. Un échec est journalisé, l'installation continue. Pas de classes VBScript, d'`Eval` ni de `with` JScript. Test : `scripts/test-msi-scripts.sh` |
| Services | `ServiceInstall` inscrit sous `HKLM\SYSTEM\CurrentControlSet\Services\<Nom>` (`ImagePath`, `Type`, `Start`, `ErrorControl`, `DisplayName`, `Description`, `ObjectName`), lu par le gestionnaire de services de Winlux et retiré à la désinstallation ; le démarrage demandé par `ServiceControl` est laissé au gestionnaire |
| Désinstallation | journal par produit dans `~/.local/share/winlux/msi/` : fichiers, dossiers créés, valeurs et clés de registre ; le paquet est conservé à côté (`<code>.msi`) quand il a des actions EXE, DLL ou script, pour rejouer celles de la désinstallation (`REMOVE="ALL"`, `Installed`) |

Dossiers standard : `C:\` est le dossier personnel ; `ProgramFiles64Folder`
= `C:\Program Files`, `ProgramFilesFolder` = `C:\Program Files (x86)`,
`ProgramMenuFolder` = `C:\AppData\Roaming\Microsoft\Windows\Start Menu\Programs`
(lu par `winlux-applications`), `DesktopFolder` = le Bureau XDG.

Codes de retour : 0, 1603 (échec), 1605 (produit inconnu), 1619 (paquet
introuvable), 1620 (paquet invalide), 1639 (ligne de commande). Linux tronque
les codes de sortie à 8 bits (1605 devient 69).

## Assistant au double-clic

Un double-clic sur un `.msi` (ou sur un installateur NSIS ou Inno Setup
reconnu par `winlux --setup --info`) ouvre `winlux-setup-wizard` : accueil
avec nom, version et éditeur (dossier modifiable et raccourci sur le Bureau
pour NSIS et Inno), progression, puis fin avec le journal en cas d'échec,
en français, anglais, allemand et espagnol. L'installation elle-même reste
silencieuse (`--msiexec /i … /qn` ou `--setup … [/D=…] [/nodesktop]`).
Sans session graphique, ou si des arguments sont passés (`/S`, `/qn`…),
`winlux-open` installe sans l'assistant. Test : `scripts/test-setup-wizard.sh`.

## Pas encore pris en charge

Requêtes SQL
`INSERT`/`UPDATE`/`DELETE` (seul `MsiViewModify` écrit), lignes temporaires
partagées entre deux actions DLL (chacune voit la base du paquet),
recherche `CompLocator`, boîtes de dialogue propres au paquet (table `Dialog`), réparation `/f`
(traitée comme une réinstallation), transformations `.mst` et correctifs `.msp`,
compression Quantum.
