# Planificateur de tâches et pare-feu

Deux outils d'administration maison, sans systemd, cron, Wine ni pare-feu tiers.
Les services font partie du runtime (`winlux --tasks …`, `winlux --firewall …`)
et sont lancés par l'init de l'ISO. Les applications graphiques sont dans le
shell Win32 (`testexe/src/shell/taskschd.rs` et `firewall.rs`), en français, anglais,
espagnol ou allemand selon Paramètres > Langue (ou `LANG`), en thème clair ou
sombre. Les appels `t(fr, en)` donnent le français et l'anglais ; l'espagnol et
l'allemand sont dans la table `TRANSLATIONS` de `admin_ui.rs` (tout nouveau
texte doit y être ajouté, sinon il s'affiche en anglais).

Ouverture : menu Démarrer (« Planificateur de tâches », « Pare-feu Winlux »),
ou les commandes `taskschd`, `taskschd.msc`, `firewall`, `wf.msc`. Dans la
session native : `winlux-shell-app taskschd|firewall`.

## Planificateur de tâches

Une tâche est un fichier Latin-1 `clé=valeur` : `/etc/winlux/tasks/<id>.task`.

```
name=Sauvegarde nocturne
folder=\
description=Copie des documents
enabled=1
trigger=daily 02:00
trigger=boot
action=C:\Outils\backup.exe<TAB>/all<TAB>C:\Outils
ac_only=1
network=0
time_limit=4320
```

Déclencheurs : `once AAAA-MM-JJ HH:MM`, `daily HH:MM [N jours]`,
`weekly HH:MM 1,3,5` (0 = dimanche), `interval N` (minutes), `boot`, `logon`,
`event network-up|network-down|power-ac|power-battery|usb-added`.

Actions : un `.exe` est lancé par le runtime, un `.sh` par `sh`, un `.py` par
`python3`, sinon la commande Linux elle-même. Les chemins Windows sont
convertis (`C:\` → dossier Windows de l'utilisateur, `Z:\` → `/`).

Service `winlux --tasks daemon` : vérifie l'heure chaque seconde (au plus
cinq minutes rattrapées après une veille), relève l'état réseau/alimentation/USB
toutes les deux secondes, n'exécute jamais deux instances d'une même tâche et
arrête une tâche qui dépasse `time_limit` minutes.

| Fichier | Contenu |
|---|---|
| `/var/lib/winlux/tasks/history.log` | `époque⇥heure⇥id⇥nom⇥type⇥détail` (`start`, `end`, `skip`, `error`, `timeout`) |
| `/var/lib/winlux/tasks/<id>.last` | dernière exécution et code de sortie |
| `/run/winlux/tasks/next.txt` | prochaine exécution de chaque tâche |
| `/run/winlux/tasks/running/<id>` | PID de l'instance en cours |

Commandes : `winlux --tasks list | run ID | end ID | enable ID | disable ID |
delete ID | fire logon|boot|ÉVÉNEMENT`.

### API Windows (`ITaskService`)

`CoCreateInstance(CLSID_TaskScheduler)` renvoie un `ITaskService` intégré au
runtime. Pris en charge : `Connect` (machine locale), `GetFolder`,
`CreateFolder`/`DeleteFolder`/`GetFolders`, `NewTask`, `RegisterTaskDefinition`,
`RegisterTask` (XML du planificateur), `GetTask`/`GetTasks`/`DeleteTask`,
`IRegisteredTask` (état, activation, `Run`, `Stop`, dernière et prochaine
exécution, XML, définition), `GetRunningTasks`, `IRegistrationInfo`,
`ITaskSettings`, `IPrincipal` (conservé, sans effet), `ITriggerCollection`,
`IActionCollection` et `IExecAction`.

Correspondance des déclencheurs : temps (avec `Repetition.Interval` → intervalle),
quotidien, hebdomadaire, démarrage, ouverture de session, enregistrement
(exécution immédiate). Limites connues : les déclencheurs sur événement du
journal Windows sont conservés mais ne se déclenchent pas ; mensuel, inactivité
et changement d'état de session sont refusés ; seules les actions « démarrer un
programme » existent ; `IEnumVARIANT` (`_NewEnum`) n'est pas fourni.

## Pare-feu

Configuration : `/etc/winlux/firewall.conf` (Latin-1).

```
profile=private
notify=1
enabled.public=1
inbound.public=block
outbound.public=allow
rule=1<TAB>Bureau à distance<TAB>in<TAB>allow<TAB>tcp<TAB>3389<TAB><TAB>domain,private<TAB>1
```

Champs d'une règle : id, nom, `in|out`, `allow|block`, `any|tcp|udp|icmp`, ports
(`80,443,8000-8100`, vide = tous), programme (vide = tous), profils, activée.

`winlux --firewall apply` remplace atomiquement la table `inet winlux_fw` :
connexions établies, boucle locale, ICMP indispensable et DHCP autorisés ; règles
de blocage avant les règles d'autorisation (comme Windows) ; puis la politique
par défaut du profil actif. Les paquets refusés sont journalisés par le noyau
(préfixe `winlux-fw-in:`/`winlux-fw-out:`, 10 par seconde au plus).

Règles par application :
- entrantes : le service relève les ports où le programme écoute
  (`/proc/net/{tcp,udp}{,6}`) et ouvre ou ferme exactement ces ports ; pour une
  application Windows, le programme est le `.exe` lancé par le runtime ;
- sortantes : les processus du programme sont placés dans le cgroup
  `/sys/fs/cgroup/winlux-fw/r<id>`, filtré par `socket cgroupv2`. Un processus
  est rattaché au plus deux secondes après son démarrage.

Service `winlux --firewall daemon` : applique la configuration (et à chaque
modification), suit `/dev/kmsg` pour `/var/lib/winlux/firewall/blocked.log`
(`époque⇥heure⇥sens⇥protocole⇥source⇥port⇥destination⇥port`), et quand une
application (hors services système de `/usr/sbin`, `/usr/lib`…) écoute sans
règle entrante, écrit l'alerte `/run/winlux/firewall/prompt.txt`. Le bureau
affiche alors une notification et la boîte « Alerte de sécurité » ;
« Autoriser l'accès » ou « Bloquer » crée la règle (`winlux --firewall answer
allow|block`) pour le profil du réseau actuel.

L'état est dans `/run/winlux/firewall/status.txt` (`backend=absent` si `nft`
manque ; l'ISO embarque le paquet `nftables`).
