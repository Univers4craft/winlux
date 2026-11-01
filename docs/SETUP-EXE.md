# Installateurs .exe (NSIS et Inno Setup)

Les installateurs Windows les plus répandus sont produits par **NSIS**
(Notepad++, 7-Zip, VLC…) et **Inno Setup** (WinMerge, ShareX, Audacity…).
Ce sont des programmes 32 bits : Winlux n'exécute pas leur code. Le runtime
reconnaît l'installateur, lit les données qu'il transporte et interprète
lui-même son script d'installation, comme le fait le msiexec intégré
(`docs/MSIEXEC.md`). Tout est écrit dans `runtime/src/setup`, sans Wine ni
bibliothèque tierce.

```sh
winlux Setup.exe                              # double-clic : installe
winlux Setup.exe /S /D=C:\Programmes\Appli    # silencieux, dossier choisi
winlux --setup --info Setup.exe               # type, nom, version, dossier
winlux --setup --list                         # applications installées
winlux --setup --uninstall WinMerge_is1       # ou winlux unins000.exe
```

Options : `/S`, `/SILENT`, `/VERYSILENT`, `--quiet` (pas de message),
`/D=dossier`, `/DIR=dossier`, `--dir=dossier`, `/nodesktop` (pas de raccourci
sur le Bureau). `WINLUX_SETUP_NATIVE=1` désactive la détection et charge
l'exe comme un programme ordinaire ; `WINLUX_SETUP_DEBUG=1` détaille le
script interprété.

## Inno Setup

| Domaine | Détail |
|---|---|
| Versions | 1.x à 7.x : 52 versions de structures, générées depuis les définitions publiées par Inno Setup (`inno_layout.rs`), ANSI et Unicode, en-têtes 32 et 64 bits |
| Compression | LZMA, LZMA2, zlib, stocké ; filtre d'appels x86 (versions 1 à 3) ; blocs à CRC |
| Logique | constantes (`{app}`, `{pf}`, `{group}`, `{cm:…}`, `{%VAR\|défaut}`, `{reg:…}`, `{code:…}` → valeur par défaut), type par défaut, composants et tâches, expressions `Check` (`IsComponentSelected`, `IsTaskSelected`, `Is64BitInstallMode`, `not`/`and`/`or`…), versions minimales de Windows, mode 64 bits |
| Actions | fichiers, dossiers, raccourcis `.lnk`, fichiers `.ini`, registre (chaînes, entiers, binaire, multi-chaînes, suppression), `UninstallDelete`, clé `Uninstall\<AppId>_is1` complète, `unins000.exe` |
| Non pris en charge | installateurs protégés par mot de passe (chiffrés), données dans des fichiers `.bin` séparés, bzip2, script Pascal (`[Code]`) : les fonctions inconnues prennent leur valeur par défaut |

## NSIS 3

| Domaine | Détail |
|---|---|
| Conteneur | en-tête `NullsoftInst`, flux solide ou blocs séparés, LZMA (avec ou sans filtre BCJ) et deflate ; ANSI et Unicode |
| Interpréteur | toutes les instructions du script : variables, piles, sauts, comparaisons, chaînes, fichiers, `SetOutPath`, `File`, `CreateShortcut`, registre, `.ini`, `WriteUninstaller`, `.onInit`, sections cochées, tables de langue (langue de l'utilisateur, sinon anglais) |
| Réponses | `MessageBox` renvoie la réponse `/SD` prévue pour le mode silencieux, sinon une réponse positive |
| Ignoré (journalisé) | appels de plugins (`System::Call`, `nsDialogs`, `UserInfo`…), `Exec`/`ExecWait`/`ExecShell` de programmes externes |
| Non pris en charge | NSIS 2 et versions modifiées de l'exehead, bzip2 |

## Désinstallation et « Programmes installés »

Chaque installation laisse un journal dans
`~/.local/share/winlux/setup/<id>.txt` : fichiers, dossiers créés, valeurs et
clés de registre ajoutées. La désinstallation (par `--uninstall`, ou en
lançant le désinstalleur de l'application, `unins000.exe` ou `uninstall.exe`)
retire tout cela et laisse le registre dans son état d'origine. Une
réinstallation remplace les fichiers et retire ceux qui ne font plus partie
de l'application.

## Fenêtre « Applications installées »

Menu Démarrer → *Applications installées* (ou `appwiz.cpl`) liste tous les
programmes Windows inscrits dans le registre : paquets `.msi`, installateurs
NSIS et Inno Setup, et programmes qui s'inscrivent eux-mêmes (clés
`Uninstall` de HKLM, HKLM\WOW6432Node et HKCU). Pour chacun : nom, éditeur,
version, type d'installateur, taille et date d'installation, avec recherche
et tri par nom, taille ou date. *Désinstaller* (puis *Confirmer*) choisit le
bon outil : msiexec intégré pour un `.msi`, journal d'installation pour ce
que Winlux a installé, sinon le désinstalleur de l'application lancé par
Winlux. La fenêtre est traduite en français, anglais, espagnol et allemand.

Le travail est fait par le runtime, utilisable aussi en ligne de commande :

```sh
winlux --setup --programs                  # une ligne app=… par application
winlux --setup --remove "HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\WinMerge_is1"
```

## Installateurs testés

| Application | Format | Résultat |
|---|---|---|
| Notepad++ 8.6.9 x64 | NSIS 3, LZMA solide | installé (programme, plugins, traduction française, raccourci, clé Uninstall), désinstallé proprement |
| WinMerge 2.16.44 x64 | Inno 5.5.7 Unicode | 138 fichiers, raccourcis en français, registre ; inscription d'extension shell ignorée |
| ShareX 16.1.0 | Inno 6.1.0 | 570 fichiers, raccourcis Démarrer, Démarrage, Envoyer vers et Bureau |
| Audacity 3.6.4 x64 | Inno 6.3.0 | 261 fichiers, raccourcis |

Chaque installation prend entre 1 et 4 secondes.
