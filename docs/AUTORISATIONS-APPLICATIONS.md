# Autorisations par application — conception

Demande de damien (11 novembre 2026) : à l'installation, une application
reçoit d'office ce dont elle a besoin (micro, caméra, son…). Ensuite, d'un
clic droit sur l'application → **Autorisations**, à côté de Copier et
Coller, on peut lui retirer finement la caméra, le micro, le son, ou le
démarrage à l'ouverture de session, qu'elle se soit inscrite avec ou sans
l'accord de la personne.

Ce document fait l'état des lieux, puis décrit le modèle cible et le
découpage en PR. **Rien de ceci n'entre dans la 0.2.21** (gel des
fonctionnalités) ; l'implémentation commence juste après la sortie.

Règles qui encadrent tout le reste :

- **Lancer une application ne pose jamais de question (#1733).** Aucune
  boîte au lancement, ni plus tard au milieu du démarrage.
- **On ne fait jamais attendre une application.** Une capacité bloquée
  répond « refusé » (ou du silence, ou une liste vide) tout de suite. Une
  demande, quand il en reste une, ne bloque pas le fil appelant (#1882 a
  montré qu'une boîte synchrone fait planter Steam).
- **Pas de réglage caché** : ce qui est accordé d'office est visible et
  retirable dans Paramètres, en quatre langues (fr, en, de, es).
- La saisie à la place de la personne (macros) reste un choix explicite,
  refusé par défaut, rejouée par XTEST, jamais par uhid.

## 1. Existant (master d1f80651)

### Moteur : `runtime/src/permissions.rs`

| Capacité (`key`) | Défaut aujourd'hui | Où elle est vérifiée | Effet d'un refus |
| --- | --- | --- | --- |
| `files` | accordée d'office, mémorisée | `paths.rs:486` (`path_allowed`), `process.rs:344` (`ask_files_before_start`) | chemin remplacé par `/proc/winlux-permission-denied/…` (« introuvable ») + une notification |
| `network` | accordée d'office | `ws2_32.rs:996`, `winhttp.rs:524` | erreur réseau |
| `sandbox` | accordée d'office | `process.rs:1030`, `sandbox_fallback.rs` | Chromium garde son bac à sable |
| `camera` | **demandée au premier usage** | `v4l2.rs:231` | aucune caméra listée |
| `microphone` | **demandée au premier usage** | `winmm.rs:812`, `audio/mod.rs:366` | silence ; winmm voit 0 micro |
| `screen` | **demandée au premier usage** | `gui.rs:4368` (`capture_screen`) | capture vide |
| `input` | demandée, **jamais implicite** (#1655) | `saisie.rs:840` | `ERROR_ACCESS_DENIED` |

- Interrupteurs globaux `/etc/winlux/privacy.conf` (`CAMERA=deny`,
  `MICROPHONE=deny`) : ils l'emportent sur tout.
- Les programmes de `/opt/winlux` et `/usr/lib/winlux` sont de confiance ;
  `WINLUX_PERMISSIONS=off` coupe tout (tests).
- Un service sans fenêtre (`WINLUX_SERVICE_NAME`) reçoit un refus mémorisé,
  sans question (#1620).
- Sans affichage ni assistant : vie privée refusée, le reste accordé, rien
  n'est mémorisé (#1038).
- **La question est synchrone** : `ask()` lance `winlux-permission ask` et
  attend son code de sortie (`Command::status()`), donc le fil de
  l'application est suspendu jusqu'à la réponse. Une décision est gardée
  pour toute la durée du processus (`SESSION`) : un changement fait dans
  Paramètres ne s'applique qu'au prochain lancement.

### Stockage

`~/.config/winlux/permissions.tsv` (ou `WINLUX_PERMISSIONS_FILE`), **par
compte**, une ligne par décision :
`capacité<TAB>allow|deny<TAB>chemin Linux canonique du .exe`.
La dernière ligne gagne ; l'écriture passe par un fichier temporaire puis
`rename`.

La clé est **le chemin exact du .exe**. Deux conséquences :

1. les applications Squirrel (Discord, Claude, Teams) s'installent dans
   `…\app-<version>\` : à chaque mise à jour le chemin change et le choix de
   la personne est perdu ;
2. une application faite de plusieurs .exe (Steam : `steam.exe`,
   `steamwebhelper.exe`, `SteamService.exe`) a autant de lignes : bloquer le
   micro de `steam.exe` laisse `steamwebhelper.exe` libre.

### Outil et interface

- `scripts/winlux-permission.py` (`/usr/bin/winlux-permission`) :
  `ask CAP NOM EXE` (fenêtre GTK, code 0/1/2), `list`, `set CAP allow|deny EXE`,
  `forget EXE`. Textes en fr/en/es/de.
- Paramètres, page 18 « Confidentialité et sécurité »
  (`winlux-settings.py`, `permission_controls`) : les deux interrupteurs
  globaux, puis une carte par .exe **déjà présent dans permissions.tsv**.
  Une application installée qui n'a encore rien utilisé n'y figure pas.
- Aucun clic droit « Autorisations ». Le clic droit sur un .exe dans
  l'Explorateur (`testexe/src/shell/explorer.rs`, `Item::ExeSettings`)
  ouvre « Réglages de l'exécutable » (GPU, processeur).

### Démarrage à l'ouverture de session

| Source | Honorée aujourd'hui ? | Où |
| --- | --- | --- |
| Services Windows `Start = 2` | oui | `winlux --services autostart` (`winlux-session-startup.py`) |
| Tâches planifiées « à l'ouverture de session » | oui | `winlux --tasks fire logon` (`build-live-iso.sh`) |
| Clés `…\CurrentVersion\Run` et `RunOnce` (HKCU, HKLM) | **non** | — |
| Dossier « Démarrage » du menu Démarrer (`.lnk`) | **non** | — |
| `~/.config/autostart/*.desktop` (programmes Linux) | oui | `winlux-session-startup.py` |

Une application Windows qui coche « Lancer au démarrage » (Discord, Steam,
Spotify, iCUE…) écrit une valeur `Run` ou un raccourci dans « Démarrage » :
Winlux l'ignore, elle ne démarre jamais seule.

## 2. Modèle cible

### 2.1 Identité d'une application

Les décisions portent sur **l'application**, pas sur un .exe. L'identité
d'un .exe est, dans cet ordre :

1. le dossier d'installation (`InstallLocation`) de l'entrée
   « Désinstaller » qui contient ce .exe ;
2. sinon le premier dossier sous `Program Files`, `Program Files (x86)`,
   `AppData\Local\Programs`, `AppData\Local` ou `AppData\Roaming` ;
3. sinon le .exe lui-même (programme portable, `Téléchargements\Setup.exe`).

Ainsi `…\Discord\app-1.0.9213\Discord.exe` et `…\Discord\Update.exe` sont
la même application « Discord », et le choix survit aux mises à jour. Le
nom affiché vient de l'entrée « Désinstaller » (`DisplayName`), sinon de la
version du .exe (`FileDescription`), sinon du nom du fichier.

### 2.2 Capacités et état par défaut

« Accordé d'office » veut dire : aucune question, l'application l'a dès
l'installation, la ligne est visible dans Paramètres et se retire d'un clic.

| Capacité | Libellé | Défaut | Effet d'un blocage, immédiat et sans attente |
| --- | --- | --- | --- |
| `microphone` | Micro | accordé d'office | aucun micro listé, ou silence si le flux est déjà ouvert |
| `camera` | Caméra | accordé d'office | aucune caméra listée ; flux ouvert → image noire |
| `sound` (nouveau) | Son | accordé d'office | le mélangeur joue cette application à volume 0 ; elle croit jouer normalement |
| `screen` | Capture de l'écran | accordé d'office | capture vide |
| `files` | Fichiers personnels | accordé d'office (inchangé) | « introuvable », comme aujourd'hui |
| `network` | Réseau et Internet | accordé d'office (inchangé) | erreur réseau |
| `autostart` (nouveau) | Démarrage à l'ouverture de session | accordé d'office, comme Windows | ses entrées Run, Démarrage, tâches « ouverture de session » et services automatiques ne sont pas lancés |
| `notifications` (nouveau, phase 5) | Notifications | accordé d'office | bulles et toasts avalés |
| `sandbox` | Lancer sans isolation | accordé d'office (inchangé), rangé sous « Avancé » | Chromium garde son bac à sable |
| `input` | Taper et piloter la souris à votre place | **refusé tant que la personne ne l'a pas autorisé** | `ERROR_ACCESS_DENIED` |

Points à trancher par damien, avec la recommandation :

- **Micro, caméra et capture d'écran passent d'« demandé au premier usage »
  à « accordé d'office »**, comme il le demande. C'est un changement de la
  règle actuelle (« la question micro vient quand le logiciel utilise
  vraiment le micro, refus par défaut »). Recommandation : suivre sa
  demande, et compenser par un **témoin dans la zone de notification**
  (micro, caméra ou écran en cours d'usage, avec le nom de l'application ;
  un clic ouvre ses autorisations), comme le fait Windows.
- **`input` reste refusé par défaut** (décision « macros » déjà prise) :
  c'est ce qui permettrait à un programme de taper à la place de la
  personne. Il ne fait plus attendre l'application (voir 2.4).

Les interrupteurs globaux de `privacy.conf` restent au-dessus de tout et
gagnent `SOUND=deny` et `SCREEN=deny`. Le démarrage automatique n'a pas
d'interrupteur global : on le retire à une application, pas à toutes.

### 2.3 Stockage

Toujours **par compte**, dans le même fichier, qui passe en version 2 :

```
# winlux-permissions 2
app<TAB>/home/damien/.wine-free/…/AppData/Local/Discord<TAB>microphone<TAB>deny
app<TAB>/home/damien/…/Program Files (x86)/Steam<TAB>autostart<TAB>deny
exe<TAB>/home/damien/Téléchargements/outil.exe<TAB>camera<TAB>deny
```

- Seules les **exceptions** sont écrites : l'absence de ligne veut dire
  « défaut du tableau 2.2 ». Une application installée apparaît dans
  Paramètres même sans ligne.
- Les anciennes lignes (`cap<TAB>décision<TAB>exe`) sont relues telles
  quelles et converties à la première écriture ; un `allow` hérité qui
  n'est que le défaut disparaît, un `deny` est rattaché à l'application.
- Le runtime relit le fichier quand sa date de modification change (un
  `stat` au plus une fois par seconde) : **un blocage fait dans Paramètres
  agit sur l'application déjà ouverte**, micro et son compris.
- Limite connue, à verser à la revue de sécurité : le fichier appartient au
  compte, donc un programme malveillant du même compte peut le réécrire, et
  peut aussi parler directement au noyau. Ces autorisations protègent la vie
  privée face à des applications ordinaires ; elles ne sont pas un bac à
  sable (#1733).

### 2.4 Plus jamais d'attente

- Capacité accordée : rien ne se passe.
- Capacité bloquée : la réponse « refusé » est immédiate.
- `input` jamais décidé : la réponse « refusé » est immédiate, et
  **en parallèle** une notification non modale propose « Autoriser
  « X » à taper à votre place ? — Autoriser / Ignorer » (refus par défaut,
  quatre langues). Si la personne autorise, l'appel suivant passe, grâce à
  la relecture du fichier. Une seule notification par application et par
  session.
- `winlux-permission ask` (synchrone) ne sert plus au runtime ; il reste
  pour les scripts.

### 2.5 Démarrage à l'ouverture de session

- Nouveau `winlux --startup run`, lancé par `winlux-session-startup.py`,
  qui lit `Run` et `RunOnce` (HKCU, puis HKLM) et les deux dossiers
  « Démarrage », et lance chaque entrée dont l'application a `autostart`.
  `--services autostart` et `--tasks fire logon` appliquent le même filtre.
- Quand une application s'inscrit au démarrage (écriture d'une valeur `Run`,
  d'un `.lnk` dans « Démarrage », d'une tâche « ouverture de session »),
  une notification non modale le dit : « « Discord » démarrera à
  l'ouverture de session — Gérer ». Pas de question : c'est une
  information, la personne garde la main.
- Page Paramètres « Démarrage » (liste de toutes les entrées, un
  interrupteur par application, l'impact n'est pas estimé).

### 2.6 Interfaces

**Clic droit → Autorisations.** Même entrée, même fenêtre, partout où l'on
voit une application :

- Explorateur (`explorer.rs`) : sur un `.exe` et sur un `.lnk` qui pointe
  vers un `.exe`, juste avant Couper, Copier, Coller (à côté de
  « Réglages de l'exécutable »).
- Menu Démarrer (`winlux-start-menu.py`) : dans le menu d'une application
  Windows (avec Épingler, Ouvrir l'emplacement du fichier, Désinstaller).
- Bureau et Thunar : action `uca.xml` sur `*.exe`, `*.lnk` et les lanceurs
  `winlux-pe-*.desktop`.
- Barre des tâches : plus tard, pas dans ce chantier.

L'entrée ouvre **Paramètres, page 43 « Autorisations de l'application »**
(`winlux-settings --page 43 --app CHEMIN`), qui reprend la forme de la page
« Options avancées » d'une application : icône, nom, éditeur, version, puis
un interrupteur par capacité du tableau 2.2, « Démarrage à l'ouverture de
session » en tête de sa propre section, « Lancer sans isolation » sous
« Avancé », et un bouton « Rétablir les autorisations par défaut ».

**Paramètres.**

- Page 39 « Applications installées » : menu « … » de chaque application →
  « Autorisations » (page 43).
- Page 18 « Confidentialité et sécurité » : les interrupteurs globaux, puis
  pour chaque capacité la liste des applications installées avec leur
  interrupteur (comme Windows liste les applications sous « Micro »).
- Page 44 « Démarrage » (Applications) : section 2.5.

Les numéros 43 et 44 sont à réserver auprès du fil Paramètres, les numéros
se télescopant entre quatre fichiers.

## 3. Découpage en PR (après la 0.2.21)

Chaque PR a son contrôle `scripts/campagne.d/NNNN-sujet.py` et, si besoin,
une étape `scripts/test-all.d/NNNN-sujet.sh`.

1. **Moteur** : identité d'application (2.1), fichier v2 et migration (2.3),
   relecture à chaud, nouveaux défauts (2.2), `input` sans attente (2.4).
   Tests unitaires : Squirrel `app-<version>`, Steam multi-exe, migration,
   blocage pris en compte sans relancer.
2. **Son** : capacité `sound` dans le mélangeur (`audio/mod.rs`), gain 0
   par processus, relu à chaud.
3. **Fenêtre et page 43**, `winlux-permission app-list | app-show | app-set`,
   textes en quatre langues.
4. **Clics droits** : Explorateur, menu Démarrer, `uca.xml`.
5. **Démarrage** : `winlux --startup run`, filtre sur services et tâches,
   notification d'inscription, page 44.
6. **Témoin d'usage** micro, caméra, écran dans la zone de notification ;
   capacité `notifications`.

## 4. Défauts relevés pendant l'état des lieux

Une issue chacun :

- #1888 : les clés `Run`/`RunOnce` et le dossier « Démarrage » sont ignorés ;
- #1889 : un choix d’autorisation est perdu à chaque mise à jour d’une application
  Squirrel, et ne vaut que pour un .exe d'une application qui en a
  plusieurs ;
- #1890 : la demande d’autorisation suspend le fil de l’application jusqu’à la
  réponse, et un choix fait dans Paramètres n'agit qu'au lancement suivant.
