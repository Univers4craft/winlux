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

`sh scripts/test-share.sh` et `sh scripts/test-transfer.sh` (dans la CI). Le
partage a aussi été vérifié de bout en bout avec un vrai `smbd` et
`smbclient` : écriture autorisée/refusée selon le droit, mauvais mot de passe
refusé, invités refusés puis acceptés.
