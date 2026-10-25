# Installateur disque : état expérimental et contrôles

`scripts/install.sh` est distinct de `integration/install.sh` : le premier
**efface un disque**, le second installe le runtime dans Linux.
Ne pas utiliser l'installateur disque sur un SSD contenant des données.

## Protections

- Disque entier uniquement : partitions isolées, volumes composites actifs,
  lecture seule, capacité insuffisante et inventaire illisible sont refusés.
- Les montages et swaps des descendants sont contrôlés, ainsi que les holders
  du noyau. Les contrôles sont répétés avant le partitionnement.
- Les signatures Btrfs, ZFS, RAID, LVM, LUKS et bcache sont refusées même si
  inactives : l'appartenance à un ensemble multi-disque ne doit pas être devinée.
- Identité, modèle, numéro de série, taille et SHA-256 de l'ISO affichés.
- Confirmation exacte `EFFACER /dev/...` depuis un terminal ; aucun `--yes`.
- `--check-only` prépare et valide la source sans écrire sur la cible.
- L'automatisation `--test-image` est limitée à un loop dont le backing-file
  correspond exactement à l'image indiquée. Elle n'accepte pas un disque physique.
- GPT avec partition BIOS boot, ESP et racine ext4 ; aucune écriture NVRAM.
- La racine complète de l'ISO est déployée, avec ses bibliothèques et BUILD-INFO.
  Les échecs de GRUB ne sont plus transformés en réussites.
- La source est extraite avant l'effacement. Ne fournir qu'une ISO de confiance :
  SHA-256 n'est pas une signature d'éditeur. `bsdtar` conserve ses protections
  par défaut contre les chemins de sortie dangereux.

Les anciennes options `--no-part` et `--label` sont volontairement refusées.
Cela ne garantit ni la sécurité face au retrait physique d'un périphérique
pendant l'opération, ni la récupération des données après formatage.

## Dépendances et validations

Les outils incluent `libarchive-tools` (bsdtar), xorriso, parted, dosfstools,
e2fsprogs et les modules GRUB BIOS/UEFI. Les outils manquants bloquent l'installation.
Le démarrage disque sans initrd exige les pilotes de stockage et ext4 intégrés
au noyau ; ce point doit être validé pour chaque cible matérielle.

Tests de refus, sans root et sans écriture de disque :

```sh
sh scripts/test-install-safety.sh
```

Test privilégié facultatif, sur une **nouvelle image temporaire** uniquement :

```sh
sudo scripts/test-install-vm.sh --iso build/winlux-validation.iso --keep-disk
```

Ce test utilise l'installateur réel puis vérifie le disque installé en BIOS et
UEFI sous QEMU sans affichage, avec délai maximal. Ce test ou son alternative
isolée ci-dessous doit réussir avant de
considérer l'installation disque comme validée. Ne pas confondre ce test avec
le démarrage Live de l'ISO, qui n'écrit pas de disque et n'exige pas root.

Alternative sans root sur l'hôte, après construction du stage ISO :

```sh
scripts/test-install-vm-guest.sh build/winlux-validation.iso
```

Elle prépare une VM d'installation isolée et un fichier disque neuf. La VM reçoit
root, pas l'hôte ; la confirmation est saisie sur sa console série. Aucun disque
physique ni dossier de l'hôte n'est partagé avec l'invité. `bsdtar` doit être
disponible sur l'hôte (ou indiqué par `WINLUX_BSDTAR`). Les artefacts temporaires
sont conservés dans `build/install-guest.*` pour diagnostic. Chaque lancement
QEMU a un délai maximal et reste sans fenêtre visible.

Les journaux d'une construction ou d'un démarrage local ne constituent pas
une publication : aucune release ne doit être créée avant validation des étapes
requises. Les tests unitaires de refus ne certifient pas un installateur de production.
Le workflow de release exige désormais les tests Live BIOS/UEFI et du disque
installé avant publication ; un noyau manquant ne déclenche plus de publication
partielle silencieuse. La portée du paquet de mise à jour reste `runtime` dans
ce workflow : l'ISO et le paquet runtime ne doivent pas être confondus.
Le tag doit aussi correspondre à `VERSION`. Le workflow modifié n'a pas été
déclenché pour publier une release lors de cette validation locale.

## Validation locale du 25 octobre 2026

- [x] Contrôles de refus et syntaxe shell : réussis.
- [x] Construction ISO complète avec noyau 7.0.0-38-generic : réussie.
- [x] Démarrage Live BIOS et UEFI : réussi, marqueurs PE séquentiels/parallèles et X11.
- [x] Installateur réel exécuté en VM, disque neuf de 4 Gio : réussi après inclusion
  des modules NLS FAT nécessaires au montage de l'ESP.
- [x] Démarrage du disque installé **sans ISO** en BIOS et UEFI : réussi, mêmes marqueurs.
- Aucun SSD physique modifié, aucune release publiée. Cela ne remplace pas une
  validation sur du matériel réel ni un test de coupure de courant.
