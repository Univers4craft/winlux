# Registre Windows et configuration Linux

Winlux possède un registre Windows persistant, écrit de zéro, partagé par
toutes les applications .exe du profil, par `reg.exe`, `regedit.exe`,
msiexec et l'éditeur du registre. Le magasin est dans
`runtime/src/registry/` ; les API Win32 (`Reg*`) sont dans
`runtime/src/dlls/advapi32.rs`. Les données sont enregistrées dans
`~/.local/share/winlux/registry.tsv` (ou le répertoire de données Winlux du
profil). `WINLUX_REGISTRY_PATH` permet aux tests de choisir un fichier isolé.
Ce n'est pas un format de ruche Windows natif.

## Format du fichier (WINLUX-REGISTRY-2)

Une ligne d'en-tête `WINLUX-REGISTRY-2`, puis des lignes séparées par des
tabulations :

- `K <chemin hex> <horodatage>` : une clé ; l'horodatage est un FILETIME.
- `V <chemin hex> <nom hex> <type> <données hex>` : une valeur.

Les chemins et les noms sont en UTF-8, avec leur casse d'origine ; ils se
comparent sans tenir compte de la casse (un lecteur externe doit les passer en
minuscules). Les chaînes (types 1, 2 et 7) sont toujours en UTF-16LE, comme
sous Windows : les API « A » les convertissent en Windows-1252. Un fichier de
l'ancien format `WINLUX-REGISTRY-1` est relu et converti à la première
écriture (chaînes ANSI/UTF-8 passées en UTF-16, clés `hkcr\…` déplacées).

Racines stockées : `hkcu`, `hklm`, `hku`, `hkcc`. **HKEY_CLASSES_ROOT n'est
pas stocké** : c'est la vue fusionnée de `hkcu\Software\Classes`
(prioritaire) et de `hklm\Software\Classes`. Une clé créée sous HKCR va
dans HKLM, sauf si elle existe déjà dans HKCU.

Chaque écriture relit le fichier sous `flock`, applique la modification et
remplace le fichier de façon atomique ; deux applis qui écrivent en même temps
ne s'écrasent pas. Les lectures gardent une copie en mémoire tant que le
fichier n'a pas changé (inode, date, taille). Une copie
`registry.tsv.bak` est faite avant chaque lancement d'appli.

## Fichiers .reg, reg.exe et regedit.exe

- Les fichiers `.reg` « Windows Registry Editor Version 5.00 » (UTF-16 ou
  UTF-8) et `REGEDIT4` (ANSI) sont lus : clés, `[-clé]`, `@=`, chaînes,
  `dword:`, `hex:`, `hex(n):`, `"valeur"=-`, lignes continuées par `\`.
  L'export écrit la version 5.00 en UTF-16 avec BOM, comme regedit.
- `regedit /s fichier.reg` importe sans question ; `regedit /e fichier.reg
  [clé]` exporte ; `regedit` seul ouvre l'éditeur graphique (un fichier passé
  sans `/s` y est proposé à l'import avec confirmation).
- `reg query|add|delete|import|export` reprend les options courantes de
  Windows (`/v`, `/ve`, `/va`, `/t`, `/d`, `/s`, `/se`, `/f`).

Ces deux commandes sont intégrées au runtime comme msiexec : un installateur
qui lance `reg.exe` ou `regedit.exe` n'a besoin d'aucun fichier externe.

## API Win32

Ouverture/création (`RegOpenKey[Ex]`, `RegCreateKey[Ex]`,
`RegOpenCurrentUser`, `RegOpenUserClassesRoot`), valeurs
(`RegSetValue[Ex]`, `RegSetKeyValue`, `RegQueryValue[Ex]`, `RegGetValue` avec
filtre de type et développement de REG_EXPAND_SZ, `RegDeleteValue`,
`RegDeleteKeyValue`), clés (`RegDeleteKey[Ex]`, `RegDeleteTree`,
`RegCopyTree`, `RegRenameKey`), énumération (`RegEnumKey[Ex]`,
`RegEnumValue`, `RegQueryInfoKey` avec la date de modification),
`RegNotifyChangeKeyValue` (synchrone ou avec événement) et
`RegLoadMUIStringW` (chaînes simples). Les versions A et W existent ; les
contrats API-set et KernelBase y sont reliés.

`RegLoadMUIStringW` ne résout pas encore les références `@fichier.dll,-id`.
Les ACL, la redirection WOW64 (`/reg:32`), les clés volatiles et les ruches
`RegLoadKey`/`RegSaveKey` ne sont pas gérées.

## Fonctions shlwapi

`runtime/src/dlls/shlwapi_reg.rs` ajoute les raccourcis de shlwapi, en
versions A et W, sur le même registre :

- valeurs : `SHGetValue`, `SHSetValue`, `SHDeleteValue`, `SHQueryValueEx`,
  `SHRegGetValue` (filtres SRRF), `SHRegGetPath`, `SHRegSetPath`. Comme sous
  Windows, une REG_EXPAND_SZ est développée et rendue en REG_SZ ;
- clés : `SHDeleteKey` (toute l'arborescence), `SHDeleteEmptyKey` (refuse une
  clé qui a encore des sous-clés ou des valeurs), `SHCopyKey`,
  `SHEnumKeyEx`, `SHEnumValue`, `SHQueryInfoKey`, `SHRegDuplicateHKey` ;
- réglages « utilisateur puis machine » : `SHRegGetUSValue`,
  `SHRegSetUSValue`, `SHRegGetBoolUSValue` (« yes », « true », 1…) et les
  clés `HUSKEY` (`SHRegOpenUSKey`, `SHRegCreateUSKey`, `SHRegQueryUSValue`,
  `SHRegWriteUSValue`, `SHRegDeleteUSValue`, `SHRegEnumUSKey`,
  `SHRegEnumUSValue`, `SHRegQueryInfoUSKey`, `SHRegCloseUSKey`) : HKCU est lu
  d'abord, puis HKLM ;
- `SHOpenRegStream2` et `SHOpenRegStream` : un `IStream` sur une valeur
  binaire, réécrite dans le registre au `Commit` ou au dernier `Release` ;
- associations de fichiers : `AssocQueryString` (commande, programme, nom du
  type, nom de l'appli, icône, type MIME, extension shell, ProgID) et
  `AssocQueryKey`, lus dans la vue fusionnée HKCR.

## Accès NT (ntdll)

Les appels bas niveau suivants réutilisent le même magasin :

- `RtlFormatCurrentUserKeyPath` alloue un chemin Unicode, libérable par
  `RtlFreeUnicodeString`.
- `NtOpenKey` et `NtOpenKeyEx` ouvrent une clé existante ; ils n'inventent pas
  les clés manquantes. Les options étendues non prises en charge sont refusées.
- `NtCreateKey` crée réellement une clé persistante, ou ouvre une clé déjà
  présente, et indique laquelle de ces opérations a eu lieu.
- `NtQueryValueKey` lit les valeurs dans les formats Basic, Full et Partial,
  avec les tailles requises et les erreurs de tampon trop petit ou tronqué.
- `NtClose` ferme aussi les handles de registre et signale un handle invalide.

Les chemins `\Registry\Machine` sont reliés à HKLM. Le chemin utilisateur
`\Registry\User\S-1-22-1-<uid Linux effectif>` est relié à HKCU. Il représente
l'identité Linux locale ; cela n'implémente ni les jetons de sécurité Windows,
ni l'impersonation, ni un domaine Active Directory. Un autre utilisateur n'est
pas redirigé vers HKCU. Les ruches restent dans le fichier du profil Winlux ;
HKLM n'est pas encore un magasin machine partagé entre tous les comptes Linux.

Les clés volatiles, les options backup/restore et les métadonnées de classe de
`NtCreateKey` ne sont pas prises en charge. Les autres formats de requête,
l'écriture NT des valeurs et les ACL Windows restent à compléter ; les fonctions
Win32 de lecture/écriture existent déjà.

## Côté Linux

Le fichier du registre peut être consulté depuis Linux et le catalogue
d'applications Winlux le lit déjà. Cela ne transforme pas les configurations
Linux en clés de registre : les paramètres système Winlux utilisent notamment
`/etc/winlux/input.conf`, `preferences.conf` et `display.conf`, et le bureau
natif utilise aussi Xfconf et les fichiers de configuration du profil.

## Éditeur du Registre

L'appli « Éditeur du Registre » (menu Démarrer, commande `regedit`, ou
`winlux --regedit`) reprend l'éditeur de Windows 11 :

- arborescence Ordinateur → HKEY_CLASSES_ROOT, HKEY_CURRENT_USER,
  HKEY_LOCAL_MACHINE, HKEY_USERS, HKEY_CURRENT_CONFIG et barre d'adresse ;
- liste des valeurs (nom, type, données) avec « (par défaut) » ;
- menu Nouveau : clé, chaîne, valeur binaire, DWORD 32 bits, QWORD 64 bits,
  chaîne multiple, chaîne extensible ;
- modification (DWORD/QWORD en hexadécimal ou décimal), renommage (F2),
  suppression (Suppr) avec confirmation ;
- recherche dans les clés, les valeurs et les données (Ctrl+F, F3 pour la
  suivante) ;
- import et export .reg d'une branche ou de tout le registre.

Un double-clic sur un fichier .reg ouvre l'éditeur, qui demande confirmation
avant d'ajouter les informations au Registre (`regedit /s` importe sans
demander, comme sous Windows). Les textes de l'appli sont traduits en
français, anglais, espagnol et allemand.

La synchronisation explicite de certains paramètres Windows/Linux reste à
développer.

## Vérifications

Le test `persiste_valeurs_binaires_et_respecte_la_casse_insensible` vérifie la
persistance, l'ouverture d'une même clé par les API Win32 et NT, la lecture
des mêmes octets, la création NT visible depuis Win32, les dispositions
créée/existante, la libération du chemin Unicode et la fermeture des handles.
Il vérifie aussi qu'un chemin utilisateur inconnu n'ouvre pas HKCU et que les
options non implémentées ne renvoient pas un faux succès.
Les lectures NT sont vérifiées dans les trois formats, y compris les tailles,
les données binaires et l'absence d'écriture au-delà du tampon fourni.
