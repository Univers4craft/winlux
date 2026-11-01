# Réseau et disques

Cette page couvre tout ce qui relie Winlux au reste du monde : le réseau, le pare-feu, le partage de dossiers, les lecteurs réseau, les clés USB et disques externes, et la partition d'un Windows déjà installé.

## Réseau

- **NetworkManager** gère l'Ethernet et le Wi-Fi (avec un repli DHCP simple s'il est absent) ; le panneau réseau de la barre des tâches et Paramètres › Réseau affichent l'état et permettent de se connecter.
- Les programmes Windows ont **Winsock** (TCP, UDP, `getaddrinfo`, `select`), **WinHTTP** et **iphlpapi**. Une connexion sortante hors de la boucle locale est autorisée d'office, mais listée et blocable dans Paramètres › Confidentialité et sécurité.
- L'installateur sait configurer le Wi-Fi et un **proxy** dès l'installation.

**En cours** (pull request #186) : Winsock IPv6 et multicast pour les serveurs ouverts par des `.exe`, page « Ports ouverts » du pare-feu nommant chaque application, et proxy système commun aux applications Windows et Linux.

## Pare-feu

Le pare-feu Winlux est maison, au-dessus de **nftables** (`winlux --firewall`, application « Pare-feu Winlux » ou `wf.msc`). Référence : [docs/PLANIFICATEUR_PAREFEU.md](https://github.com/Univers4craft/winlux/blob/master/docs/PLANIFICATEUR_PAREFEU.md).

- **Profils** Public, Privé et Domaine, comme sous Windows ; par défaut, l'entrant est bloqué sur un réseau public.
- **Règles** par port, protocole, programme et profil (`/etc/winlux/firewall.conf`). Les règles de blocage passent avant les autorisations, comme sous Windows.
- **Règles par application** : en entrée, le service ouvre exactement les ports où le programme écoute ; en sortie, les processus du programme sont placés dans un cgroup filtré.
- **Alerte de sécurité** : quand une application commence à écouter sans règle, le bureau affiche la boîte « Autoriser l'accès / Bloquer », qui crée la règle pour le profil du réseau actuel.
- Les paquets refusés sont journalisés (`/var/lib/winlux/firewall/blocked.log`).

## Partager un dossier avec Windows, Mac et Linux

Dans l'Explorateur : clic droit sur un dossier › **Partager ce dossier…**. Le dossier devient visible sous `\\NOM-DU-PC\Dossier` pour les PC Windows et `smb://NOM-DU-PC` pour les Mac. Référence : [docs/PARTAGE.md](https://github.com/Univers4craft/winlux/blob/master/docs/PARTAGE.md).

- **Serveur** : Samba, piloté par `scripts/winlux-share.sh`. Samba et wsdd sont masqués au démarrage et ne démarrent que lorsqu'un dossier est partagé. Sans Samba, la fonction est cachée.
- **Comptes** : chaque compte Winlux se connecte avec son nom et son mot de passe. Droits **Aucun accès / Lecture / Modification** par compte et pour les invités.
- **Sécurité** : SMB1 désactivé, NTLMv2 seulement, dossiers système refusés, ports ouverts **sur les réseaux privés seulement**.
- **Découverte** : wsdd fait apparaître le PC dans le panneau Réseau de Windows ; Avahi l'annonce au Finder ; le module `fruit` de Samba gère les métadonnées macOS.
- **Windows 11 24H2+** refuse les sessions invitées : il faut s'y connecter avec un compte Winlux (Paramètres le rappelle).
- **Test automatisé** : la CI lance le vrai serveur Samba et s'y connecte en simulant un client Windows 11 et un Mac (`scripts/test-share-clients.sh`).

### Transférer ses réglages vers un autre PC Winlux

Paramètres › Partage et transfert › **Enregistrer** crée un fichier `.wlxprofile` (thème, barre des tâches, fond, épingles, accessibilité, lecteurs réseau sans mot de passe, applications par défaut, langue, clavier, souris, confidentialité). Sur l'autre PC : **Importer**, puis rouvrir la session. Aucun compte en ligne ; un bouton « Annuler le dernier import » restaure l'état précédent.

## Lecteurs réseau

Paramètres › **Lecteurs réseau** monte un stockage distant comme un disque avec une lettre (`E:`, `F:`…), visible dans « Ce PC » et par tous les programmes Windows et Linux.

| Protocole | Outil Linux utilisé |
|---|---|
| SMB / CIFS | `mount.cifs` (ou `rclone` en repli) |
| NFS | `mount.nfs` |
| SFTP | `sshfs` |
| FTP / FTPS | `rclone` |
| WebDAV(S) | `rclone` |
| S3 | `rclone` |

- Seuls les protocoles dont l'outil est présent sont proposés ; la page explique quoi installer pour les autres.
- Les **mots de passe sont chiffrés** (ChaCha20-Poly1305, implémentation maison dans `runtime/src/netdrive/seal.rs`), jamais stockés en clair.
- Reconnexion automatique et état hors ligne publiés dans le registre commun des montages (`mounts.tsv`), lu par l'Explorateur.
- Commande : `winlux-drives` (lien vers `winlux --drives`).

## Clés USB, disques externes et partition Windows

Le service `winlux --volumes daemon` monte automatiquement, avec une lettre de lecteur, tout volume branché. Les programmes Windows le voient comme `E:\`, les programmes Linux sous `/media/<utilisateur>/<nom du volume>`. Référence : [docs/VOLUMES.md](https://github.com/Univers4craft/winlux/blob/master/docs/VOLUMES.md).

| Système de fichiers | Pilote | Écriture |
|---|---|---|
| NTFS | `ntfs3` (noyau), sinon `ntfs-3g` | oui, sauf volume laissé ouvert par Windows |
| FAT32 / FAT16 | `vfat` | oui |
| exFAT | `exfat` | oui |
| ext4 | `ext4` | oui (disques amovibles seulement) |
| BitLocker | — | jamais : un message explique pourquoi |

- **Même volume, même lettre** (mémorisée par numéro de série).
- Partitions EFI, réservées, de récupération, swap ou cachées sont ignorées, comme sous Windows.
- **Windows en veille prolongée ou en démarrage rapide** (`hiberfil.sys`) : le volume reste en lecture seule et l'utilisateur est invité à arrêter Windows en maintenant Maj, pour ne pas perdre de données.
- **Volume « sale »** : lecture seule, et la Gestion des disques propose un bouton **Réparer**.
- Commandes : `winlux --volumes list`, `scan`, `eject E:`, `repair E:`. `/etc/winlux/volumes.conf` permet de couper le montage automatique.

## Images disque et archives

- Une image disque (`.iso`, `.img`…) s'ouvre dans l'Explorateur et se monte comme un lecteur.
- Les archives **ZIP, 7z, RAR, tar, tar.gz** s'ouvrent comme des dossiers, avec « Extraire tout » ; ZIP, gzip et tar sont décodés par le runtime, 7z et RAR via `bsdtar` ([docs/ARCHIVES.md](https://github.com/Univers4craft/winlux/blob/master/docs/ARCHIVES.md)).

## Impression

CUPS est intégré. Paramètres › Imprimantes et scanners ajoute les imprimantes ; les programmes Windows impriment par `winspool`/`comdlg32` maison (rendu PDF envoyé à CUPS par IPP), et une imprimante virtuelle « Imprimer en PDF (Winlux) » est toujours disponible.

## Bureau à distance

Un **serveur RFB (VNC) écrit en Rust** (`runtime/src/remote/`) permet de prendre la main sur la session depuis un client VNC, réglable dans Paramètres › Bureau à distance ([docs/SYSTEM-SERVICES.md](https://github.com/Univers4craft/winlux/blob/master/docs/SYSTEM-SERVICES.md)).

Voir aussi : [Chemins et droits](Chemins-et-droits) · [Paramètres](Parametres)
