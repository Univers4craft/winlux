# Installation

> **Préversion technique.** Découvrez Winlux d'abord dans une machine virtuelle. Avant d'installer sur un vrai PC, **sauvegardez vos données**.

## Ce qu'il faut

| | Minimum conseillé |
|---|---|
| Processeur | x86-64 (64 bits) ; KVM conseillé pour une VM fluide |
| Mémoire | 4 Gio |
| Disque | 24 Gio (l'installateur refuse moins de 20 Gio pour Winlux) |
| Démarrage | BIOS ou UEFI, Secure Boot activé ou non |
| Affichage | de 1024×600 à la 4K ; l'installateur s'adapte à la définition et au DPI |

## 1. Télécharger l'ISO

Sur la page de la [dernière release](https://github.com/Univers4craft/winlux/releases/latest), téléchargez `winlux-live-<version>.iso` et son fichier `.sha256`, puis vérifiez :

```sh
sha256sum -c winlux-live-v0.2.12.iso.sha256
```

## 2a. Dans une machine virtuelle

| Logiciel | Réglages |
|---|---|
| **QEMU/KVM** (virt-manager, GNOME Boîtes) | 4 Gio de RAM, 2 à 4 processeurs, disque virtio de 24 Gio, BIOS ou UEFI (OVMF). Affichage « VGA standard » (`bochs`) ou QXL. |
| **VirtualBox** | Type « Linux / Ubuntu (64-bit) », 4 Gio, 24 Gio, contrôleur graphique VMSVGA. |
| **VMware** | « Ubuntu 64 bits », 4 Gio, 24 Gio (pilote `vmwgfx`). |

Les pilotes d'affichage `bochs`, `qxl` et `vmwgfx` sont dans l'image, et Xorg utilise le pilote générique `modesetting`. En VM, Paramètres › Écran propose « Suivre la taille de la VM » pour que le bureau prenne la taille de la fenêtre.

Pour les développeurs, `scripts/create-test-vm.sh` crée et démarre une VM QEMU persistante (disque QCOW2 de 24 Gio, 4 Gio, 4 processeurs, 1920×1080) ; voir [Construire l'ISO](Construire-l-ISO).

## 2b. Sur une clé USB

L'ISO est hybride : elle s'écrit telle quelle sur une clé USB (avec Balena Etcher, Ventoy, ou en ligne de commande) :

```sh
sudo dd if=winlux-live-v0.2.12.iso of=/dev/sdX bs=4M status=progress conv=fsync
```

> Remplacez `/dev/sdX` par la clé, **pas** par un disque. `dd` efface la cible sans demander.

Démarrez le PC sur la clé (touche F12, F11, F8 ou Échap selon le fabricant). Avec Secure Boot activé, Winlux démarre sans réglage grâce au shim signé ([docs/SECURE-BOOT.md](https://github.com/Univers4craft/winlux/blob/master/docs/SECURE-BOOT.md)).

## 3. La session Live

Le menu GRUB (thème Prisme) lance la session Live : le bureau s'ouvre directement, tout tourne en mémoire, **rien n'est écrit sur le disque**. C'est le bon moment pour vérifier que l'écran, le réseau, le son et vos programmes fonctionnent. Le menu GRUB propose aussi une vérification de l'intégrité du média et le mode récupération.

## 4. L'installateur

Lancez **« Installer Winlux »** depuis le bureau. Les étapes principales :

1. **Langue et clavier** : français, anglais, espagnol, allemand, italien, portugais ; dispositions AZERTY, QWERTY, QWERTZ…
2. **Réseau** : Wi-Fi et proxy si besoin.
3. **Disque** :
   - par défaut, **installation automatique sur tout le disque** choisi ;
   - à côté d'un **Windows existant** (double démarrage) : l'installateur réduit la partition Windows en gardant une marge, et le menu de démarrage propose les deux systèmes ;
   - le **mode avancé** donne un partitionnement manuel complet (créer, supprimer, redimensionner, formater) ;
   - **chiffrement LUKS** en option : la phrase de passe est demandée au démarrage avec la disposition clavier choisie.
4. **Compte** : compte **local** (nom, mot de passe, avatar). Aucun compte en ligne.
5. **Import** : récupération des documents d'une partition Windows, ou migration depuis un autre PC par le réseau.
6. **Performances** : test rapide du matériel et choix d'un profil d'utilisation.
7. **Résumé** puis installation.

Pour déployer plusieurs postes, un **fichier de réponses** YAML ou JSON pré-remplit l'installateur et peut le rendre entièrement silencieux ([docs/AUTOINSTALL.md](https://github.com/Univers4craft/winlux/blob/master/docs/AUTOINSTALL.md)).

## 5. Premier démarrage

L'assistant **Bienvenue** propose : thème clair ou sombre, fond d'écran, position de la barre des tâches, choix des applications à installer, et une courte visite (lancer un `.exe`, installer les logiciels courants en un clic). Il peut être relancé depuis Démarrer.

## Récupération

Le menu GRUB contient une entrée de secours (**« Winlux Recovery »** sur la clé USB, touche `r` ; **« Winlux secours »** sur un système installé, touche `s`) :

1. Réparer le démarrage (GRUB BIOS et UEFI, chaîne Secure Boot) ;
2. Restaurer le point de restauration précédent (`winlux-update --rollback`) ;
3. Terminal de secours ;
4. Vérifier le système de fichiers (`e2fsck`) ;
5. Redémarrer / éteindre.

Détails : [docs/RECOVERY.md](https://github.com/Univers4craft/winlux/blob/master/docs/RECOVERY.md).

## Dépannage

| Problème | Que faire |
|---|---|
| Écran noir en VM | Choisir un adaptateur graphique pris en charge (QEMU : VGA standard ou QXL ; VirtualBox : VMSVGA ; VMware : SVGA). |
| Le PC refuse de démarrer la clé avec Secure Boot | Mettre à jour le micrologiciel, ou désactiver Secure Boot ; voir aussi l'enregistrement d'une clé MOK. |
| La partition Windows est en lecture seule | Windows est en veille prolongée ou en démarrage rapide : l'arrêter en maintenant Maj. |
| Un `.exe` ne s'ouvre pas | Voir [Compatibilité des applications](Compatibilite-des-applications) et signaler le problème depuis Paramètres. |

## Mettre à jour une installation existante

Voir [Publication et mises à jour](Publication-et-mises-a-jour). En résumé : une installation **v0.2.6 ou plus ancienne** doit être réinstallée une fois avec une ISO récente ; une installation **v0.2.12** devra l'être une fois avec l'ISO v0.2.13 ; ensuite, tout se fait depuis Paramètres › Mises à jour.

## Utiliser seulement le runtime sur un Linux existant

Sans installer Winlux, le runtime peut lancer des `.exe` sur une distribution Linux x86-64 : voir [Comment contribuer](Comment-contribuer) pour la compilation, puis `sudo integration/install.sh` pour le double-clic. Ce mode n'apporte pas le bureau Winlux.
