# Chemins et droits

Un programme Windows parle en `C:\Users\Léa\Documents\rapport.docx` ; Linux en `/home/lea/Documents/rapport.docx`. Winlux traduit dans les deux sens, applique les règles de nom de Windows, et relie les droits Windows (jetons, SID, ACL, UAC) aux droits Linux. Références : [docs/CHEMINS-ET-DROITS.md](https://github.com/Univers4craft/winlux/blob/master/docs/CHEMINS-ET-DROITS.md), code dans `runtime/src/paths.rs`, `dlls/knownfolders.rs`, `dlls/kernel32_fs.rs`, `dlls/winfile.rs`, `dlls/security.rs`, `uac.rs`.

## Lecteurs et profil

| Vu par le programme Windows | Côté Linux |
|---|---|
| `C:\` | dossier personnel de l'utilisateur (`WINLUX_DRIVE_C` pour changer) |
| `C:\Users\<nom>` | dossier personnel (`WINLUX_HOME` pour changer) |
| `C:\Users\<nom>\Documents`, `Desktop`, `Downloads`, `Music`, `Pictures`, `Videos` | dossiers XDG de `~/.config/user-dirs.dirs` (`~/Bureau`, `~/Téléchargements`… en français) |
| `C:\Users\<nom>\AppData\…` | `~/AppData/…` |
| `C:\Users\Public` | dossier public XDG |
| `D:` à `Y:` | clés USB, disques, partitions Windows, images disque et lecteurs réseau (voir [Réseau et disques](Reseau-et-disques)) |
| `Z:\` | racine Linux `/` |
| `\\serveur\partage` | lecteur réseau monté correspondant, sinon introuvable |
| `\\localhost\c$` | `C:` |
| lettre inconnue | introuvable (**jamais** `C:` par défaut) |

Dans l'autre sens, un fichier de `~/Bureau` est présenté au programme comme `C:\Users\<nom>\Desktop\…`. Les liens symboliques ne sont pas suivis dans la présentation : un programme retrouve le chemin qu'il a utilisé.

`SHGetFolderPath`, `SHGetKnownFolderPath`, `GetUserProfileDirectory` et les variables `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `ProgramFiles`, `SystemRoot`… sont calculés à partir du compte Linux et ajoutés à l'environnement au lancement. Une variable déjà définie côté Linux l'emporte.

## Règles de nom

- `/` et `\` sont équivalents ; `//` et `.` sont ignorés ; `..` est résolu sur le texte ; les points et espaces finaux sont retirés (sauf avec `\\?\`).
- **La casse est ignorée** : le nom exact d'abord, sinon le premier nom par ordre alphabétique qui correspond.
- Un nom Linux contenant un caractère interdit sous Windows (`" * : < > ? \ |`, caractère de contrôle, point ou espace final) est présenté au programme avec ces caractères décalés dans la zone privée Unicode (U+F000 + code, comme Cygwin), puis retraduit à l'ouverture.
- `NUL`, `CONIN$`, `CONOUT$` sont des périphériques.
- **Pas de limite `MAX_PATH`** : chemins longs avec ou sans `\\?\` (`RtlAreLongPathsEnabled` renvoie vrai). Seule reste la limite Linux de 255 octets par nom.

## Attributs des fichiers

| Attribut Windows | Correspondance Linux |
|---|---|
| Lien symbolique | `FILE_ATTRIBUTE_REPARSE_POINT` + `IO_REPARSE_TAG_SYMLINK` ; `RemoveDirectory` supprime le lien, pas la cible |
| Lecture seule | aucun bit d'écriture, ou fichier non inscriptible par l'utilisateur ; posée par `SetFileAttributes`, elle bloque l'écriture même pour root |
| Caché | nom commençant par un point |
| Propriétaire d'un fichier créé | l'utilisateur qui lance le programme, mode selon son `umask` (644 en général) |

## Sécurité Windows

- **Jeton du processus** : l'utilisateur Linux d'uid N devient `S-1-5-21-<machine>-N`, root devient SYSTEM. Groupes Tout le monde, Utilisateurs, Utilisateurs authentifiés, et Administrateurs une fois élevé. Un membre de `sudo`/`wheel`/`admin` a, avant élévation, un jeton « limité », comme un administrateur Windows.
- **Descripteurs de sécurité** (`GetFileSecurity`, `GetNamedSecurityInfo`, `GetAce`…) : propriétaire, groupe et DACL tirés des bits `rwx` Linux (propriétaire `rwx` = contrôle total ; « autres » = Tout le monde ; SYSTEM a le contrôle total).
- `CheckTokenMembership`, `IsUserAnAdmin`, `AllocateAndInitializeSid`, `ConvertSidToStringSid`, `LookupAccountSidW`, `AdjustTokenPrivileges`…

## UAC : demander les droits administrateur

Quand un programme le demande (manifeste `requireAdministrator`, `highestAvailable` pour un administrateur, ou verbe `runas` de `ShellExecute`), Winlux affiche la fenêtre **« Voulez-vous autoriser cette application à apporter des modifications à votre appareil ? »** (`scripts/winlux-uac.py`, en FR/EN/ES/DE).

- Un administrateur tape **son propre mot de passe** ; un compte standard tape celui d'un administrateur.
- **Accepté** : le programme reçoit un jeton administrateur (`WINLUX_ELEVATED=1`, hérité par ses enfants), mais il **reste lancé sous le compte de l'utilisateur** : les fichiers qu'il crée lui appartiennent.
- **Refusé** : le programme ne démarre pas (code 1223).
- Sans écran : lancement sans élévation, avec un avertissement dans le journal. `WINLUX_UAC=allow|deny` force la réponse pour les tests.

## Autorisations des applications

Comme sous Windows 11, un `.exe` demande la première fois qu'il veut accéder à quelque chose de sensible. Le choix est retenu par programme (`~/.config/winlux/permissions.tsv`) et se change dans **Paramètres › Confidentialité et sécurité**.

| Autorisation | Où c'est contrôlé | Demandée ? |
|---|---|---|
| Fichiers personnels (Bureau, Documents, Images, Musique, Vidéos, Téléchargements) | traduction des chemins ; un refus donne « fichier introuvable » | oui |
| Caméra | énumération V4L2 | oui |
| Micro | waveIn, DirectSound, WASAPI ; un refus donne du silence | oui |
| Capture de l'écran | capture du runtime | oui |
| Réseau et Internet | `connect`/`sendto` hors boucle locale, WinHTTP | non : autorisé d'office, mais listé et blocable |

- Restent libres : le dossier du programme lui-même et les fichiers qu'on lui a explicitement donnés (double-clic, glisser-déposer).
- Les interrupteurs « Caméra / Micro pour toutes les applications » l'emportent sur les choix par programme.
- Les programmes livrés avec Winlux ne demandent rien.
- Ce n'est pas un antivirus : aucun fichier n'est analysé (l'antivirus est volontairement hors périmètre).

## Comptes

Les comptes sont **locaux uniquement**. Le compte personnel est créé par l'installateur ou au premier démarrage, avec un avatar choisi parmi ceux fournis. L'écran de verrouillage vérifie le mot de passe de ce compte. Il n'y a ni compte en ligne, ni synchronisation dans un nuage.

## Ce qui reste à faire

- Faire tourner le bureau de l'ISO en utilisateur non privilégié, avec une autorisation ciblée pour les actions système (#13).
- Partage d'objets nommés et de fichiers entre processus invités, héritage des handles.

Voir aussi : [Registre Windows](Registre-Windows) · [Réseau et disques](Reseau-et-disques)
