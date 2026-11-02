---
title: Créer la clé USB
description: Préparez une clé USB qui démarre Winlux sur n'importe quel PC, avec le créateur de clé fourni.
---

## Avec le créateur de clé USB

Le créateur de clé USB de Winlux télécharge la dernière image, la vérifie et l'écrit sur la clé. Il existe pour un PC sous Windows (`.exe`) et pour Linux ou Winlux (`.deb`). Récupérez-le sur la page [Télécharger](page:download).

![Le créateur de clé USB de Winlux](img:createur-cle-usb.jpg "Le créateur de clé USB : choisir l'image, choisir la clé, créer.")

1. **Image de Winlux** : laissez « Télécharger la dernière version », ou choisissez une image ISO déjà téléchargée.
2. **Clé USB** : choisissez la clé dans la liste. Seules les clés USB et les cartes SD sont proposées, jamais le disque du PC.
3. **Créer la clé USB** : une fenêtre rappelle le nom et la taille de la clé. Rien n'est effacé avant que vous cliquiez sur « Effacer et créer la clé ».
4. Attendez la fin de l'écriture puis de la vérification : la clé est relue et comparée à l'image.

> [!warning]
> **Tout le contenu de la clé est effacé.** Copiez ailleurs ce que vous voulez garder avant de commencer.

## Avec un autre outil

L'image de Winlux est « hybride » : n'importe quel outil qui copie une image disque sur une clé USB convient, et Ventoy la démarre aussi. Sur Linux, en ligne de commande, en remplaçant `/dev/sdX` par la clé (et surtout pas par un disque) :

```sh
sudo dd if=winlux-live-v0.2.15.iso of=/dev/sdX bs=4M status=progress conv=fsync
```

## Démarrer le PC sur la clé

1. Branchez la clé et allumez (ou redémarrez) le PC.
2. Appuyez tout de suite sur la touche du menu de démarrage : souvent **F12**, **F11**, **F8** ou **Échap**, selon le fabricant.
3. Choisissez la clé USB. Elle démarre en UEFI comme en BIOS ancien, et Secure Boot peut rester activé. Sur une tablette à démarrage 32 bits (Atom), désactivez Secure Boot : la clé démarre aussi dans ce mode.
4. Le menu Winlux s'affiche : choisissez la session Live pour essayer, ou l'installation.

> [!tip]
> La session Live tourne entièrement en mémoire : c'est le bon moment pour vérifier que l'écran, le Wi-Fi, le son et vos programmes fonctionnent, sans rien changer à votre PC.

Pour aller plus loin : [Installation dans le wiki](wiki:Installation).
