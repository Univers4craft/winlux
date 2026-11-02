# Claude Code dans Winlux

But : faire tourner Claude Code dans Winlux pour que Claude puisse tester le
système de l'intérieur, sur un vrai PC. Deux chemins existent :

1. **La version Windows (`claude.exe`) par la couche PE maison** : c'est le
   chemin recommandé. Elle sert en même temps de test de compatibilité,
   puisqu'elle sollicite un runtime JavaScript complet (Bun), le TLS, les
   processus enfants, les tubes, la console et le système de fichiers.
2. **La version Linux native** : c'est la solution de repli.

Rien n'est ajouté à l'ISO. Claude Code (environ 335 Mo) s'installe après le
premier démarrage.

## Installer

**Chemin normal, sans commande à taper** : dans le menu Démarrer, ouvrir
**Claude Code** (catégorie Développement).

- Au premier lancement, une fenêtre télécharge Claude Code avec une barre de
  progression. Elle vérifie la somme SHA-256 donnée par le manifeste officiel,
  puis ouvre Claude Code dans le terminal Winlux.
- Aux lancements suivants, Claude Code s'ouvre directement.
- Le clic droit sur l'entrée propose « Mettre à jour Claude Code ».
- En cas d'échec réseau, la fenêtre propose « Réessayer ». Un téléchargement
  annulé ne laisse aucun fichier partiel.

L'application est `integration/native/claude-code/winlux-claude-code.py`,
avec son entrée `integration/native/applications/winlux-claude-code.desktop`
et son icône `winlux-claude-code.svg`. Tout est copié dans l'image avec les
autres composants du bureau natif.

**Chemin de secours**, dans un terminal :

```
winlux-claude-code install          # version Windows, par la couche PE
winlux-claude-code install linux    # repli : version Linux native
claude                              # lancer Claude Code
```

Les deux chemins téléchargent la version du canal `stable` depuis le dépôt de
diffusion officiel d'Anthropic. Ils rangent le binaire dans
`~/.local/share/winlux/claude-code/` et créent la commande
`~/.local/bin/claude`. La variable `WINLUX_CLAUDE_CANAL=latest` permet de
choisir la dernière version publiée plutôt que la stable.

Au premier lancement, Claude Code ouvre la connexion au compte dans le
navigateur.

## État vérifié (2 novembre 2026, claude.exe 2.1.316 et 2.1.309)

Les essais ont été faits sur le runtime compilé depuis cette branche, dans
un conteneur Linux sans VM. L'API Anthropic était remplacée par un faux
serveur local, qui enchaîne les outils
Write, Bash, Read puis renvoie leurs résultats.

| Fonction | État |
|---|---|
| `claude --version` | OK |
| Mode `-p` (non interactif) | OK, quatre tours complets |
| Outil Write (écriture atomique : fichier temporaire puis renommage) | OK |
| Outil Read | OK |
| Outil Bash (`cat`, `uname -s`, `pwd`) | OK, la sortie revient à Claude |
| Instantané de shell, suivi du dossier courant | OK |
| ripgrep intégré (Claude se relance sous le nom `rg`) | OK |
| HTTPS réel vers api.anthropic.com, via un proxy et une autorité de certification ajoutée | OK : une fausse clé reçoit bien « Invalid API key » |
| Interface interactive dans un terminal (pty) | OK : affichage, saisie, flèches, Entrée, dialogues, tour complet Write/Bash/Read |
| Fonctions Windows manquantes | Aucune : le journal `manquants.log` reste vide sans `WINLUX_MISSING=ignore` |

Ce qui reste à valider sur la vraie machine :

- La connexion OAuth réelle dans le navigateur. Les identifiants sont rangés
  par CredWriteW dans `~/.local/share/winlux/identifiants/`, en droits 0600.
- Une longue session, avec redimensionnement du terminal et Ctrl+C.
- Le terminal de la session native (xfce4-terminal).

## Ce que la couche PE a dû apprendre

- **Réseau (Bun/libuv)** : imports par ordinal de ws2_32 (et alias wsock32),
  SIO_BASE_HANDLE avec socket non bloquante, SO_PROTOCOL_INFO (libuv
  bascule alors sur son mode `select`), `recv` de zéro octet immédiat.
- **Fichiers** : NtQueryInformationFile, NtSetInformationFile,
  NtQueryDirectoryFile, NtQueryVolumeInformationFile, points d'analyse
  (liens symboliques), FILE_APPEND_DATA, dossiers ouverts avec
  BACKUP_SEMANTICS, LockFileEx.
- **Processus enfants** : de vrais tubes nommés (`\\.\pipe\…`) sur des
  paires de sockets Unix, et RegisterWaitForSingleObject pour apprendre la
  fin d'un enfant.
  - Les lectures overlapped se terminent tout de suite quand des données
    sont là, comme sous Windows.
  - Un hEvent marqué du bit 0 ne poste rien au port de complétion.
  - La ligne de commande exacte est transmise aux enfants Windows.
- **Console** : handles standard réels, ce qui évite que Bun masque -11 en
  63 bits. SetConsoleMode règle termios.
  - ReadConsoleInputW, PeekConsoleInputW et GetNumberOfConsoleInputEvents
    lisent le terminal.
  - SIGWINCH devient un WINDOW_BUFFER_SIZE_EVENT.
  - Les fonctions de curseur sont traduites en séquences VT, et CONIN$ et
    CONOUT$ sont pris en charge.
- **Git Bash** : Claude Code exige Git Bash sous Windows. Winlux lui fournit
  le bash Linux.
  - Au démarrage d'un programme Windows, le runtime crée le lien
    `~/.local/share/winlux/git/bin/bash.exe` → `/usr/bin/bash` et règle
    `CLAUDE_CODE_GIT_BASH_PATH` si rien d'autre ne l'est.
  - Un programme Linux nommé `.exe` se lance directement.
  - Les scripts qu'il reçoit voient leurs chemins Windows (`C:\…`) et MSYS
    (`/c/…`) traduits.
  - `pwd -P >| fichier` y écrit un chemin MSYS, comme le ferait Git Bash.
- **Divers** : `C:\Users` existe (c'est le dossier des dossiers personnels),
  et les Cred* sont pris en charge.
  - Autres fonctions ajoutées : GetGeoInfoW, GetCurrencyFormatEx,
    EnumSystemLocalesW, K32EnumProcessModules, les tables de fonctions
    croissantes (RtlAddGrowableFunctionTable) et WaitOnAddress.

## Limites connues

- `fs.watch` (ReadDirectoryChangesW) n'est pas encore pris en charge. Claude
  Code le détecte et s'en passe.
- ConPTY (CreatePseudoConsole) renvoie E_NOTIMPL : les programmes retombent
  sur des tubes.
- Les commandes passent par le bash Linux. Les outils Windows
  (`cmd.exe`, PowerShell) ne sont pas fournis.

## Repli : version Linux native

Winlux repose sur Ubuntu 26.04 : le binaire officiel `linux-x64` de Claude
Code est un exécutable Linux ordinaire. `winlux-claude-code install linux`
l'installe de la même façon. Ce chemin n'a pas d'intérêt pour tester la
couche Windows, mais il permet de travailler si une régression de la couche
PE bloquait la version Windows. Il n'a pas été exécuté pendant ces essais :
c'est le binaire Linux standard, sans rien de propre à Winlux.
