# Chemins Windows ↔ Linux et droits des fichiers

Code : `runtime/src/paths.rs` (traduction), `runtime/src/dlls/knownfolders.rs`
(dossiers spéciaux), `runtime/src/dlls/kernel32_fs.rs` (liens, variables),
`runtime/src/dlls/winfile.rs` (attributs). Suivi : issue #175.

## Lecteurs et profil

| Vu par l'appli | Côté Linux |
|---|---|
| `C:\` | dossier personnel (`WINLUX_DRIVE_C` pour changer) |
| `C:\Users\<nom>` | dossier personnel (`WINLUX_HOME` pour changer) |
| `C:\Users\<nom>\Documents`, `Desktop`, `Downloads`, `Music`, `Pictures`, `Videos` | dossiers XDG de `~/.config/user-dirs.dirs` (`~/Bureau`, `~/Téléchargements`… en français) |
| `C:\Users\<nom>\AppData\…` | `~/AppData/…` (même dossier que `C:\AppData`) |
| `C:\Users\Public` | dossier public XDG |
| `Z:\` | racine `/` |
| `D:` à `Y:` | images disque et lecteurs réseau (`mounts.tsv`) |
| `\\serveur\partage` | lecteur réseau monté dont la source correspond, sinon introuvable |
| `\\localhost\c$` | `C:` |
| lettre inconnue | introuvable (jamais `C:` par défaut) |

Dans l'autre sens, un fichier de `~/Bureau` est présenté comme
`C:\Users\<nom>\Desktop\…`. Les liens symboliques ne sont pas suivis : une
appli retrouve le chemin qu'elle a utilisé.

`SHGetFolderPath`, `SHGetKnownFolderPath`, `GetUserProfileDirectory`,
`USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `ProgramFiles`, `SystemRoot`… sont
calculés à partir du compte Linux et ajoutés à l'environnement au lancement
(une variable déjà définie côté Linux l'emporte).

## Règles de nom

- `/` et `\` équivalents, `//` et `.` ignorés, `..` résolu sur le texte,
  points et espaces finaux retirés (sauf avec `\\?\`).
- Casse ignorée : nom exact d'abord, sinon premier nom par ordre alphabétique.
- Un nom Linux contenant `" * : < > ? \ |`, un caractère de contrôle ou un
  point/espace final est présenté avec ces caractères dans la zone privée
  Unicode (U+F000 + code, comme Cygwin) et retraduit à l'ouverture.
- `NUL`, `CONIN$`, `CONOUT$` sont des périphériques.
- Pas de limite `MAX_PATH` : chemins longs avec ou sans `\\?\`
  (`RtlAreLongPathsEnabled` renvoie vrai). Limite Linux : 255 octets par nom.

## Attributs et droits

- Lien symbolique : `FILE_ATTRIBUTE_REPARSE_POINT` (+ `IO_REPARSE_TAG_SYMLINK`
  dans `FindFirstFile`) ; `RemoveDirectory` supprime le lien, pas la cible.
- Lecture seule : aucun bit d'écriture, ou fichier non inscriptible par
  l'utilisateur. Posée par `SetFileAttributes`, elle bloque l'écriture même
  pour root ; retirée, elle rend l'écriture au seul propriétaire.
- Fichiers créés : propriétaire = utilisateur qui lance l'appli, mode selon
  son `umask` (644 en général).
- Caché : nom commençant par un point.
