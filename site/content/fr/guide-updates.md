---
title: Mettre à jour
description: Les nouvelles versions de Winlux s'installent depuis Paramètres, avec un point de restauration et un retour arrière automatique.
---

## Depuis Paramètres

1. Ouvrez **Paramètres › Mises à jour › Mise à jour OS**.
2. Cliquez sur **« Rechercher des mises à jour »**. Si une version plus récente existe, la page affiche « Mise à jour disponible », sa taille et ses nouveautés.
3. Cliquez sur **« Télécharger et installer »** et suivez la progression.
4. Redémarrez quand cela vous arrange : Winlux ne force jamais le redémarrage.

![La page Mise à jour OS](img:mises-a-jour.jpg "Paramètres › Mises à jour › Mise à jour OS : recherche, pause, installation automatique, historique et retour à la version précédente.")

Sur la même page, **« Installer automatiquement les nouvelles versions »** les installe sans intervention, en dehors de vos heures d'activité et jamais pendant un jeu ; **« Suspendre les mises à jour »** les met en pause. Les paquets du système de base se mettent à jour depuis la ligne **« Logiciels et pilotes Linux »** de la même page.

## Ce qui protège votre système

- Chaque mise à jour est téléchargée depuis les versions publiées du projet ; la signature de sa description, sa taille et sa somme SHA-256 sont contrôlées avant l'installation.
- Un **point de restauration** est créé juste avant ; les trois derniers sont gardés.
- Si le bureau ne démarre plus correctement après une mise à jour, Winlux **revient tout seul** à la version précédente.
- Vos documents, vos programmes installés et vos réglages sont conservés.

> [!note]
> **Vous venez d'une version ancienne ?** Une installation en v0.2.12 ou plus ancienne doit être réinstallée une fois avec une image récente : son outil de mise à jour ne peut pas se corriger lui-même. À partir de la v0.2.13, tout se fait depuis Paramètres.

## Revenir en arrière

Si quelque chose ne va pas après une mise à jour, redémarrez et choisissez **« Winlux secours »** dans le menu de démarrage (touche `s`), puis « Restaurer le point de restauration précédent ».

Pour aller plus loin : [Publication et mises à jour](wiki:Publication-et-mises-a-jour).
