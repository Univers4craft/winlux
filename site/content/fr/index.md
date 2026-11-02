---
title: Accueil
headline: Les programmes Windows, d'un double-clic, sur Linux
lead: Winlux est un système d'exploitation libre sur noyau Linux. Vos fichiers `.exe` s'ouvrent d'un double-clic, dans un bureau qui ressemble à celui que vous connaissez, sans machine virtuelle cachée et sans couche de compatibilité empruntée.
description: Winlux, le système libre sur noyau Linux qui ouvre les programmes Windows d'un double-clic. Téléchargement, guide d'installation et applications testées.
---

## Ce que Winlux fait pour vous

::: cards
### Un double-clic suffit
Un `.exe` téléchargé s'ouvre comme n'importe quel fichier, depuis le bureau ou l'Explorateur. Les installateurs `setup.exe` et `.msi` ajoutent leur raccourci au menu Démarrer.

### Un bureau familier
Menu Démarrer centré ou à gauche, barre des tâches, Explorateur, Paramètres, Gestionnaire des tâches : on retrouve ses repères dès la première minute.

### Vos fichiers, partout
Votre dossier personnel est le lecteur `C:` pour les programmes Windows. Documents, Images et Téléchargements sont les mêmes pour toutes vos applications.

### Fait maison
La compatibilité Windows est écrite de zéro, en Rust, pour Winlux. Un seul système tourne sur votre PC : pas de Windows caché dans une machine virtuelle.

### Votre vie privée
Comptes locaux uniquement, aucune télémétrie. Un programme demande votre accord avant d'utiliser la caméra, le micro ou vos dossiers personnels.

### Quatre langues
Le système, l'installateur et ce site existent en français, en anglais, en allemand et en espagnol.
:::

## À quoi ça ressemble

![Le bureau de Winlux avec le menu Démarrer ouvert](img:bureau-menu-demarrer.jpg "Le bureau de Winlux, fond « Winlux Nuit », menu Démarrer ouvert.")

::: figure-row
![L'assistant Bienvenue au premier démarrage](img:assistant-bienvenue.jpg "Au premier démarrage, l'assistant Bienvenue règle fond, couleurs et barre des tâches.")

![La page Mises à jour des Paramètres](img:mises-a-jour.jpg "Les mises à jour s'installent depuis Paramètres, avec retour arrière automatique.")
:::

## Par où commencer

::: steps-grid
### 1. Télécharger
Récupérez l'image de la dernière version et, si vous le souhaitez, le créateur de clé USB. [Page Télécharger](page:download)

### 2. Suivre le guide
Six étapes, de la clé USB à votre premier programme Windows, puis les mises à jour. [Ouvrir le guide](page:guide)

### 3. Vérifier vos logiciels
Ce qui fonctionne déjà, ce qui ne fonctionne pas encore, et comment nous le signaler. [Applications testées](page:apps)
:::

## Où en est le projet

Winlux est une **préversion technique** : le bureau, l'installateur, les mises à jour et beaucoup de programmes Windows 64 bits fonctionnent, mais tous les logiciels ne s'ouvrent pas encore. Les programmes 32 bits, les jeux récents en Direct3D 11 et les applications .NET de bureau (WinForms, WPF) font partie des chantiers en cours. La liste complète est tenue à jour dans le [wiki](wiki:Limites-et-problemes-connus).

Ce site s'adresse à celles et ceux qui découvrent Winlux. Pour comprendre comment il est construit, lisez le [wiki technique](wiki:Home).
