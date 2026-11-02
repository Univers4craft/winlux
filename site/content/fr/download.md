---
title: Télécharger Winlux
description: Récupérez l'image d'installation de la dernière version, le créateur de clé USB, et vérifiez que votre PC convient.
---

{{download}}

Les fichiers viennent directement de la page des versions publiées du projet sur GitHub. Le nom de l'image ressemble à `winlux-live-v0.2.15.iso` ; elle pèse environ 2 Go.

## Ce qu'il faut

| | Minimum conseillé |
|---|---|
| Processeur | PC 64 bits (x86-64) |
| Mémoire | 4 Go |
| Disque | 24 Go libres (20 Go au strict minimum) |
| Démarrage | UEFI ou BIOS ancien, Secure Boot activé ou non |
| Écran | de 1024 × 600 jusqu'à la 4K |
| Clé USB | 4 Go ou plus, pour installer sur un vrai PC |

## Trois façons d'essayer

::: cards
### Dans une machine virtuelle
La façon la plus sûre de découvrir Winlux : rien ne touche à votre PC. Voir la section [machine virtuelle](#essayer-dans-une-machine-virtuelle) ci-dessous.

### Depuis une clé USB, sans installer
La session « Live » démarre Winlux en mémoire : vous testez l'écran, le réseau et le son, et rien n'est écrit sur le disque. [Créer la clé](page:guide-usb)

### Installé sur le PC
Seul sur le disque, ou à côté d'un Windows existant avec un menu pour choisir au démarrage. [Installer Winlux](page:guide-install)
:::

## Essayer dans une machine virtuelle

| Logiciel | Réglages conseillés |
|---|---|
| virt-manager, GNOME Boîtes (QEMU/KVM) | 4 Go de mémoire, 2 à 4 processeurs, disque de 24 Go, affichage « VGA standard » ou QXL |
| VirtualBox | Type « Linux / Ubuntu (64-bit) », 4 Go, 24 Go, contrôleur graphique VMSVGA |
| VMware | « Ubuntu 64 bits », 4 Go, 24 Go |

Démarrez la machine virtuelle sur le fichier ISO, puis suivez [l'installation](page:guide-install) comme sur un vrai PC. Dans Paramètres › Écran, l'option « Suivre la taille de la VM » adapte le bureau à la fenêtre.

## Vérifier le fichier téléchargé

Le créateur de clé USB vérifie l'image tout seul. Si vous l'utilisez autrement, comparez sa somme SHA-256 au fichier `.sha256` publié à côté. Sur Linux :

```sh
sha256sum -c winlux-live-v0.2.15.iso.sha256
```

Les deux sommes doivent être identiques ; sinon, téléchargez l'image à nouveau.

> [!note]
> Vous avez déjà Winlux installé ? Vous n'avez rien à télécharger ici : les nouvelles versions s'installent depuis [Paramètres › Mises à jour](page:guide-updates).
