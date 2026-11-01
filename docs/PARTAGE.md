# Partage de dossiers et transfert des réglages

Deux fonctions sans aucun compte en ligne (issue #171), réunies dans
**Paramètres > Partage et transfert** (page 28, `shell/sharing.rs`).

## Partager un dossier sur le réseau

Dans l'Explorateur : clic droit sur un dossier > **Partager ce dossier…**.
Le dossier devient visible sous `\\NOM-DU-PC\Dossier` pour les PC Windows,
les Mac (`smb://NOM-DU-PC`) et les autres Linux.

- **Serveur** : Samba (`smbd`), l'outil standard de Linux pour SMB. Winlux ne
  fait que le piloter (`scripts/winlux-share.sh`, appelé par
  `winlux-control share-*`). Si Samba n'est pas installé, l'état publie
  `available=0` : l'entrée du menu et la section de la page sont cachées.
- **Comptes** : chaque compte Winlux (`/etc/winlux/users.conf`) se connecte
  avec son nom et son mot de passe Winlux. Un compte sans mot de passe ne
  peut pas s'authentifier en SMB : il n'a accès qu'aux partages ouverts aux
  invités.
- **Droits** : par partage et par compte, Aucun accès / Lecture /
  Modification ; même choix pour les invités (sans mot de passe). Par
  défaut, le propriétaire du dossier modifie, les autres comptes lisent,
  aucun invité. Les fichiers créés à distance appartiennent au propriétaire
  du dossier (`force user`).
- **Refusé** : dossiers du système (`/`, `/etc`, `/usr`, `/var`, `/proc`…),
  `/tmp` et `/run` (sauf `/run/media`).
- **Sécurité** : SMB1 désactivé (`server min protocol = SMB2_10`), NTLMv2
  seulement. Le pare-feu Winlux ouvre 445/5357 (TCP) et 3702/5353 (UDP) sur
  les profils Privé et Domaine uniquement (règles 9445 et 9446) ; sur un
  réseau public la page l'indique.
- **Découverte** : `wsdd` (WS-Discovery) fait apparaître le PC dans le
  panneau Réseau de l'Explorateur Windows ; Avahi, s'il est présent, l'annonce
  au Finder ; le module `fruit` de Samba gère les métadonnées macOS.

- **Windows 11** (24H2 et après) exige la signature SMB et refuse les
  sessions invitées : depuis un PC Windows, il faut se connecter avec un
  compte Winlux. La page Paramètres et le panneau de partage le rappellent
  dès qu'un partage est ouvert aux invités. Un Mac, lui, accepte les invités.
- **Mac** : métadonnées Finder et fourches de ressources restent en attributs
  étendus (`fruit:resource = stream`), aucun fichier `._` n'apparaît dans le
  dossier côté Linux ; `catia` traduit les caractères interdits sous Windows.

Fichiers : registre `/etc/winlux/shares.tsv`, configuration générée
`/etc/winlux/samba/smb.conf` (le `smb.conf` du paquet l'inclut, l'original est
gardé en `smb.conf.winlux-orig`), base de comptes
`/var/lib/winlux/samba/private`, état `/run/winlux/shares.txt`. Le serveur est
relancé à l'ouverture de session (`winlux-share ensure`).

## Transférer ses réglages vers un autre PC Winlux

**Enregistrer** crée `Reglages Winlux <PC> <date>.wlxprofile` dans Documents ou
sur une clé USB. Sur l'autre PC, la page liste les fichiers `.wlxprofile`
trouvés dans Documents, Bureau, Téléchargements et sur les clés USB :
**Importer**, puis fermer et rouvrir la session.

Contenu (`scripts/winlux-transfer.sh`) : personnalisation (thème, accent, fond,
barre des tâches), épingles et icônes du bureau, accessibilité, lecteurs
réseau **sans mot de passe** (ils restent scellés sur la machine d'origine et
sont redemandés), applications par défaut (`mimeapps.list`), et les clés
système THEME, TASKBAR_ALIGN, XKB_LAYOUT, TAP_TO_CLICK, MOUSE_SPEED, LANG et
les choix de confidentialité.

L'import ne fait confiance à rien : seuls les membres attendus de l'archive
sont extraits, les liens symboliques et les fichiers trop gros sont ignorés,
les clés système sont filtrées une par une et leurs valeurs vérifiées. Avant
chaque import, les réglages actuels sont sauvegardés ; **Annuler le dernier
import** les remet.

## Tests

`sh scripts/test-share.sh` et `sh scripts/test-transfer.sh` (dans la CI).

`sudo sh scripts/test-share-clients.sh` (dans la CI aussi) démarre un vrai
Samba et le confronte à des clients qui se comportent comme :

- **Windows 11** : SMB 3.1.1 seul, signature obligatoire, NTLMv2 ; liste des
  partages, nom de partage accentué, chemin de plus de 200 caractères, droit
  Modification / Lecture respecté, mauvais mot de passe et SMB1 refusés,
  session invitée refusée (comme Windows 11).
- **macOS** : SMB 3, accès invité, fourche de ressources écrite et relue,
  pas de fichier `._`.
- **Découverte Windows** : sonde WS-Discovery (UDP 3702) puis lecture des
  métadonnées (HTTP 5357), comme le panneau Réseau de l'Explorateur.

### À vérifier sur un vrai PC Windows et un vrai Mac

Ce que le banc ne peut pas prouver (issue #171) :

1. Windows 11 : le PC Winlux apparaît dans **Réseau** de l'Explorateur.
2. Windows 11 : `\\NOM-DU-PC\Dossier` s'ouvre avec le compte Winlux ; copier
   un gros fichier (plus de 1 Go) dans les deux sens.
3. Windows 11 : « Connecter un lecteur réseau » survit à un redémarrage des
   deux PC.
4. Mac : le PC apparaît dans la barre latérale du Finder (Bonjour) ;
   `smb://NOM-DU-PC` s'ouvre en invité et avec un compte.
5. Mac : copier un dossier avec étiquettes de couleur, les étiquettes restent.
6. Pare-feu en profil **Public** : le partage n'est plus joignable.
