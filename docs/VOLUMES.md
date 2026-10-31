# Disques branchés : clés USB, disques externes et partitions Windows

Winlux monte tout seul, avec une lettre de lecteur, les volumes branchés sur
le PC : clé USB, disque externe, ou la partition d'un Windows déjà installé
(double démarrage). Les applis Windows les voient comme `E:\`, les applis
Linux sous `/media/<utilisateur>/<nom du volume>`.

Service : `winlux --volumes daemon` (root, lancé au démarrage à côté du
pare-feu). Code : `runtime/src/volumes.rs`.

## Ce qui est monté

| Système de fichiers | Pilote | Écriture |
|---|---|---|
| NTFS | `ntfs3` (noyau), sinon `ntfs-3g` | oui, sauf volume laissé ouvert par Windows |
| FAT32 / FAT16 | `vfat` | oui |
| exFAT | `exfat` | oui |
| ext4 | `ext4` | oui (droits Linux d'origine) ; disques amovibles seulement |
| BitLocker | — | jamais : un message explique pourquoi |

Sans pilote pour un système de fichiers, ses volumes ne sont pas proposés.

Ignorés comme sous Windows : partitions EFI, réservées Microsoft,
récupération, swap, partitions GPT marquées « cachée » ou « pas de lettre de
lecteur », volumes déjà montés ou utilisés (LVM, chiffrement).

Le même volume (numéro de série) retrouve la même lettre
(`/var/lib/winlux/volumes/letters.tsv`).

## Droits

Montage avec `uid`/`gid` de l'utilisateur de la session active
(`/run/winlux/sessions.txt`), `umask=022` (les autres comptes lisent),
`nosuid,nodev`, et `windows_names` pour NTFS : un nom de fichier interdit
sous Windows (`aux`, `a:b`…) est refusé, le disque reste lisible par Windows.

## Volume laissé ouvert par Windows

Tout volume est d'abord monté en lecture seule, puis vérifié :

- `hiberfil.sys` commence par `hibr` : Windows est en veille prolongée ou a
  été arrêté avec le **démarrage rapide**. Écrire perdrait des données : le
  volume reste en lecture seule et l'utilisateur est invité à arrêter Windows
  en maintenant Maj.
- Indicateur « sale » de `$Volume` (lu directement sur le disque) : Windows
  n'a pas fermé le volume proprement. Lecture seule ; **Gestion des disques**
  affiche alors un bouton **Réparer** (`ntfsfix -d`), qui remonte le volume
  en écriture.
- Échec de montage : message invitant à lancer `chkdsk /f` sous Windows.

Les messages arrivent dans le centre de notifications du bureau Winlux
(`/run/winlux/volumes/notices.txt`, traduit FR/EN/ES/DE par le bureau) et par
`notify-send` sur un bureau Linux.

## Commandes

```
winlux --volumes daemon      # service
winlux --volumes scan        # un passage
winlux --volumes list        # E:  libellé  /dev/sdb1  pilote  ro|rw  raison
winlux --volumes eject E:
winlux --volumes repair E:
```

Gestion des disques écrit `mount /dev/X`, `umount /dev/X` ou
`repair /dev/X` dans `/run/winlux/disk-action.txt`, lu par le service.

`/etc/winlux/volumes.conf` : `automount=0` coupe le montage automatique,
`internal=0` le limite aux disques amovibles.

Test en VM ou conteneur : `WINLUX_VOLUMES_EXTRA=loop` accepte les
périphériques boucle.
