---
title: Logiciels et jeux compatibles
description: La liste des logiciels et des jeux Windows qui s'installent et se lancent sur Winlux, et de ceux qui sont encore en cours.
---

Chaque logiciel de cette page a été installé puis lancé sur Winlux, dans sa version Windows officielle, telle qu'on la télécharge chez son éditeur. Aucun n'a été modifié pour l'occasion.

> [!note]
> Ces essais sont faits au banc de test, en machine virtuelle, sur la version en cours de développement. Certains correctifs arriveront avec la prochaine version publiée.

## Logiciels qui fonctionnent

Ils s'installent (ou se décompressent pour les versions portables), s'ouvrent et s'utilisent normalement.

| Logiciel | À quoi il sert | Remarque |
|---|---|---|
| VLC | Lecteur vidéo et audio | |
| Audacity | Enregistrement et montage audio | Installateur Inno Setup |
| ScreenToGif | Enregistrement de l'écran en animation | Application .NET |
| 7-Zip | Archives et compression | Le gestionnaire de fichiers fonctionne ; `7z.exe` en console liste bien les archives mais signale une erreur en sortant ([#1479](issue:1479)) |
| Notepad++ | Éditeur de texte | Installateur NSIS, installation et désinstallation propres |
| SumatraPDF | Lecteur PDF | Version portable |
| Flow Launcher | Lanceur de programmes et recherche | Application .NET |
| PuTTY | Terminal SSH | Versions 64 bits et 32 bits |
| FileZilla | Transfert de fichiers FTP et SFTP | |
| WinSCP | Transfert de fichiers SFTP | Version 32 bits |
| Claude Code | Assistant de programmation en ligne de commande | |

## Jeux qui fonctionnent

| Jeu | Genre | Graphismes |
|---|---|---|
| SuperTuxKart 1.5 | Course | OpenGL, en fenêtre et en plein écran |
| Warzone 2100 4.7 | Stratégie en temps réel | OpenGL, Vulkan et Direct3D 11 |
| Xonotic | Tir à la première personne | OpenGL, dans ses deux versions |

## Installation vérifiée, lancement pas encore

Ces logiciels s'installent et se désinstallent proprement, mais leur utilisation n'a pas encore été vérifiée en détail.

| Logiciel | À quoi il sert |
|---|---|
| WinMerge | Comparaison de fichiers et de dossiers |
| ShareX | Captures et partage d'écran |

## En cours

Ces logiciels avancent, mais ne sont pas encore utilisables de bout en bout. Chaque blocage a son issue sur GitHub.

| Logiciel | Où il en est |
|---|---|
| Paint.NET | Il s'installe et démarre son runtime .NET ; sa fenêtre principale n'est pas encore utilisable ([#138](issue:138)) |
| Client Claude pour Windows | Il démarre, se réduit et se ferme ; l'agrandir ou déplacer sa fenêtre ne fonctionne pas encore ([#1476](issue:1476), [#1477](issue:1477)) |
| Steam | Il s'installe et met à jour son client, mais n'est pas encore utilisable ([#285](issue:285), [#1440](issue:1440)) |
| Photoshop | Pas encore testé jusqu'au bout |
| IrfanView | Pas encore ([#488](issue:488)) |

Ce qui ne fonctionne pas encore, type de programme par type de programme, est expliqué sur la page [Applications testées](page:apps).

## Votre logiciel n'est pas dans la liste ?

Essayez-le ! S'il ne s'ouvre pas, Winlux vous propose de le signaler, avec le nom de la fonction qui manque. Voir [Obtenir de l'aide](page:guide-help#signaler-un-probleme).
