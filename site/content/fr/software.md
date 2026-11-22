---
title: Logiciels et jeux
description: Les logiciels et les jeux Windows qui s'installent et se lancent dans Winlux, vérifiés un par un, et ceux qui sont encore en cours.
---

Chaque logiciel de cette page a été téléchargé sur le site officiel de son éditeur, dans sa version Windows, puis installé et lancé dans Winlux sur notre banc de test. La liste est mise à jour à chaque vérification.

> [!note]
> Liste établie le 22 novembre 2026, d'après la version publiée v0.2.21 et les vérifications faites depuis. Certaines lignes concernent la prochaine version, la v0.3, pas encore publiée.

## Logiciels qui fonctionnent

| Logiciel | Catégorie | Vérifié |
|---|---|---|
| VLC | Multimédia | S'installe, lit vidéos et musique |
| Audacity 3.6 | Multimédia | S'installe et se désinstalle proprement, s'ouvre et s'utilise |
| ScreenToGif | Multimédia | S'ouvre et enregistre l'écran |
| OBS Studio | Multimédia | Ouvre sa fenêtre avec l'aperçu ([#2262](issue:2262), [#2313](issue:2313)) |
| Paint.NET 5.1 | Image | S'installe (Winlux récupère le paquet complet chez l'éditeur), s'ouvre et dessine ; le téléchargeur de l'installeur web ne va pas encore au bout ([#2752](issue:2752)) |
| 7-Zip (64 et 32 bits) | Fichiers | S'installe, ouvre et crée des archives, en fenêtre comme en console |
| Notepad++ 8.6 | Documents | S'installe et se désinstalle proprement, édite des fichiers |
| SumatraPDF 3.6 | Documents | Ouvre et affiche les PDF |
| Flow Launcher | Outils | S'installe et recherche des programmes et des fichiers |
| PuTTY (64 et 32 bits) | Réseau | Se lance et ouvre des sessions SSH |
| FileZilla | Réseau | S'installe et transfère des fichiers en FTP et SFTP |
| WinSCP (32 bits) | Réseau | Se lance et transfère des fichiers en SFTP |
| Claude Code | Développement | S'installe et fonctionne dans le terminal |
| Pythonwin | Développement | Ouvre sa fenêtre principale ([#1785](issue:1785)) |

## Jeux qui fonctionnent

| Jeu | Affichage | Vérifié |
|---|---|---|
| Warzone 2100 4.7 | Direct3D 11 ou OpenGL | Se lance et se joue |
| SuperTuxKart 1.5 | OpenGL | Se lance et se joue, environ 116 images par seconde au banc |
| Xonotic | OpenGL | Se lance et se joue, plus de 220 images par seconde au banc |

## Installés proprement, utilisation pas encore vérifiée

| Logiciel | Vérifié |
|---|---|
| WinMerge 2.16 | Installation et désinstallation propres |
| ShareX 16.1 | Installation et désinstallation propres |

## En cours

| Logiciel | Où on en est |
|---|---|
| Office | L'outil de déploiement télécharge Office et vérifie ses signatures ; l'installation ne va pas encore au bout ([#2740](issue:2740)) |
| Adobe Acrobat Reader | S'installe ; l'application ne s'utilise pas encore ([#236](issue:236)) |
| Teams | S'installe et démarre, sans atteindre l'écran de connexion ([#2511](issue:2511), [#2726](issue:2726)) |
| Adobe Creative Cloud | L'installeur démarre et joint les serveurs de l'éditeur, sa fenêtre ne s'affiche pas encore ([#2784](issue:2784)) |
| OneDrive | L'installeur ne démarre pas encore ([#2851](issue:2851)) |
| Claude pour Windows | S'installe et s'ouvre ; les défauts d'agrandissement et de déplacement sont corrigés, l'ensemble reste à revérifier |
| Steam | S'installe et télécharge son client, ne se connecte pas encore ([#285](issue:285)) |
| IrfanView | Pas encore essayé jusqu'au bout ([#488](issue:488)) |
| Photoshop | Pas encore essayé jusqu'au bout |

## Votre logiciel ou votre jeu n'est pas dans la liste ?

Essayez-le ! S'il ne s'ouvre pas, Winlux vous propose de le signaler avec le nom de la fonction qui manque, et c'est ce qui nous permet de le faire fonctionner. Voir [Obtenir de l'aide](page:guide-help#signaler-un-probleme) et, pour le détail par type de programme, [Applications](page:apps).
