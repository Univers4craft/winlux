---
title: Installer Winlux
description: L'installateur vous guide pas à pas, du choix de la langue jusqu'au redémarrage, seul sur le disque ou à côté de Windows.
---

Lancez **« Installer Winlux »** depuis le bureau de la session Live, ou choisissez l'installation dans le menu de la clé. L'installateur s'adapte à la taille de votre écran.

![L'étape Réseau de l'installateur](img:installateur-reseau.jpg "L'installateur de Winlux, étape Réseau : Wi-Fi et proxy si besoin.")

## Les étapes

1. **Langue et clavier** : français, anglais, allemand, espagnol et d'autres ; dispositions AZERTY, QWERTY, QWERTZ…
2. **Réseau** : connectez le Wi-Fi si besoin ; un proxy d'entreprise peut être indiqué.
3. **Type d'installation et disque** : voir ci-dessous.
4. **Importer depuis Windows** : récupérez fond d'écran, favoris et documents d'une partition Windows, ou migrez depuis un autre PC par le réseau.
5. **Profil d'utilisation** : un court test du matériel, puis bureautique, jeu, développement ou serveur. Winlux règle le processeur, la mémoire et la carte graphique en conséquence.
6. **Compte** : un compte **local**, avec nom, mot de passe et image. Aucun compte en ligne n'est demandé.
7. **Résumé** : relisez tout, puis cliquez sur « Installer ».

## Choisir où installer

| Choix | Ce qui se passe |
|---|---|
| Tout le disque (par défaut) | Winlux prend tout le disque choisi. Son contenu est effacé. |
| À côté de Windows | L'installateur réduit la partition Windows en gardant une marge ; au démarrage, un menu propose les deux systèmes. |
| Mode avancé | Partitionnement manuel : créer, supprimer, redimensionner, formater. |
| Chiffrement | En option : la phrase de passe est demandée à chaque démarrage. |

![Le choix du profil d'utilisation](img:installateur-profil.jpg "Le profil d'utilisation règle le système selon votre usage ; il reste modifiable ensuite.")

> [!warning]
> Si Windows est installé sur le PC, arrêtez-le complètement avant d'installer Winlux à côté (maintenez Maj en cliquant sur Arrêter) : sinon sa partition reste verrouillée par le démarrage rapide.

À la fin, retirez la clé USB et redémarrez. L'installateur ne propose jamais comme destination la clé dont le PC a démarré.

Pour aller plus loin : [Installation dans le wiki](wiki:Installation) · [installation silencieuse de plusieurs postes](repo:docs/AUTOINSTALL.md).
