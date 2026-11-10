---
title: Logiciels et jeux
description: Les logiciels et les jeux Windows qui s'installent et se lancent dans Winlux, vérifiés un par un, et ceux qui sont encore en cours.
---

Chaque logiciel de cette page a été installé depuis son site officiel, dans sa version Windows, puis lancé et utilisé dans Winlux sur notre banc de test. La liste est mise à jour à chaque vérification.

> [!note]
> Liste établie le 10 novembre 2026. Certaines lignes concernent la prochaine version de test, pas encore publiée.

## Logiciels qui fonctionnent

| Logiciel | Catégorie | Vérifié |
|---|---|---|
| VLC | Multimédia | S'installe, lit vidéos et musique |
| Audacity 3.6 | Multimédia | S'installe et se désinstalle proprement, s'ouvre et s'utilise |
| ScreenToGif | Multimédia | S'ouvre et enregistre l'écran |
| 7-Zip (64 et 32 bits) | Fichiers | S'installe, ouvre et crée des archives ; en console, `7z.exe` sort encore avec le code 5 ([#1479](issue:1479)) |
| Notepad++ 8.6 | Documents | S'installe et se désinstalle proprement, édite des fichiers |
| SumatraPDF 3.6 | Documents | Ouvre et affiche les PDF |
| Flow Launcher | Outils | S'installe et recherche des programmes et des fichiers |
| PuTTY (64 et 32 bits) | Réseau | Se lance et ouvre des sessions SSH |
| FileZilla | Réseau | S'installe et transfère des fichiers en FTP et SFTP |
| WinSCP (32 bits) | Réseau | Se lance et transfère des fichiers en SFTP |
| Claude Code | Développement | S'installe et fonctionne dans le terminal |

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
| Claude pour Windows | S'installe, s'ouvre, se réduit et se ferme ; le bouton « Agrandir » ([#1476](issue:1476)) et le déplacement par la barre de titre ([#1477](issue:1477)) ne fonctionnent pas encore |
| Paint.NET 5.1 | S'installe et se désinstalle ; l'ouverture de la fenêtre principale est en cours de vérification ([#138](issue:138)) |
| Steam | S'installe et télécharge son client, ne se connecte pas encore ([#285](issue:285)) et plante parfois en se fermant ([#1440](issue:1440)) |
| IrfanView | Pas encore essayé jusqu'au bout ([#488](issue:488)) |
| Photoshop | Pas encore essayé jusqu'au bout |

## Votre logiciel ou votre jeu n'est pas dans la liste ?

Essayez-le ! S'il ne s'ouvre pas, Winlux vous propose de le signaler avec le nom de la fonction qui manque, et c'est ce qui nous permet de le faire fonctionner. Voir [Obtenir de l'aide](page:guide-help#signaler-un-probleme) et, pour le détail par type de programme, [Applications](page:apps).
