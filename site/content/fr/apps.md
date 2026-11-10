---
title: Applications testées
description: Ce qui fonctionne déjà dans Winlux, ce qui ne fonctionne pas encore, et comment nous aider à élargir la liste.
---

Winlux ouvre les programmes Windows grâce à une couche de compatibilité écrite de zéro. Beaucoup de programmes fonctionnent déjà, mais pas tous : cette page dit les choses franchement.

## Par type de programme

| Type de programme | État aujourd'hui |
|---|---|
| Programme 64 bits simple (outils, utilitaires, programmes en ligne de commande) | Fonctionne en général |
| Installateur `.msi`, NSIS ou Inno Setup | Fonctionne |
| Application .NET qui embarque son propre runtime | Démarre, mais son interface ne s'ouvre pas toujours |
| Interfaces WinForms et WPF | Partiel : ScreenToGif et Flow Launcher (WPF) fonctionnent, beaucoup d'autres pas encore |
| Jeux Direct3D 9, 11 et 12 | Partiel, à valider jeu par jeu |
| Programme 32 bits | Partiel : PuTTY, 7-Zip et WinSCP en 32 bits fonctionnent, beaucoup d'autres ne s'ouvrent pas encore |
| Pilotes, antivirus, anti-triche au niveau du système | Non pris en charge |
| Applications en paquet MSIX ou AppX | Installation, raccourcis et désinstallation |

## Logiciels essayés

La liste des logiciels et des jeux vérifiés un par un, avec ce qui fonctionne et ce qui est encore en cours, est sur la page [Logiciels et jeux](page:software).

Le détail fonction par fonction et l'ordre des prochains chantiers sont dans le wiki : [Compatibilité des applications](wiki:Compatibilite-des-applications).

## Ce qui arrive ensuite

Par ordre d'impact sur le nombre de logiciels débloqués : les fonctions Windows les plus demandées, OpenGL et Vulkan pour les applications, les bibliothèques Visual C++ redistribuables, les connexions sécurisées HTTPS, les shaders Direct3D, puis les programmes 32 bits. La [feuille de route](wiki:Feuille-de-route) suit l'avancement.

## Votre logiciel n'est pas dans la liste ?

Essayez-le ! S'il ne s'ouvre pas, Winlux vous propose de le signaler, avec le nom de la fonction qui manque : c'est exactement ce qui nous permet de le faire fonctionner. Voir [Obtenir de l'aide](page:guide-help#signaler-un-probleme).
