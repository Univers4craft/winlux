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
| Désinstallation | journal par produit dans `~/.local/share/winlux/msi/` : fichiers, dossiers créés, valeurs et clés de registre |

Dossiers standard : `C:\` est le dossier personnel ; `ProgramFiles64Folder`
= `C:\Program Files`, `ProgramFilesFolder` = `C:\Program Files (x86)`,
`ProgramMenuFolder` = `C:\AppData\Roaming\Microsoft\Windows\Start Menu\Programs`
(lu par `winlux-applications`), `DesktopFolder` = le Bureau XDG.

Codes de retour : 0, 1603 (échec), 1605 (produit inconnu), 1619 (paquet
introuvable), 1620 (paquet invalide), 1639 (ligne de commande). Linux tronque
les codes de sortie à 8 bits (1605 devient 69).

## Pas encore pris en charge

Actions personnalisées DLL, EXE ou script (journalisées puis ignorées),
services Windows, variables d'environnement, fichiers `.ini`, recherche
`AppSearch`, interface graphique d'installation (`/qf`), réparation `/f`
(traitée comme une réinstallation), transformations `.mst` et correctifs `.msp`,
compression Quantum.
