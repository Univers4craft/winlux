---
title: Lancer un programme Windows
description: Double-cliquez sur un .exe, installez un logiciel avec son installateur, et réglez ses autorisations.
---

## Double-cliquer, tout simplement

Double-cliquez sur un fichier `.exe` dans l'Explorateur ou sur le bureau : il s'ouvre directement. Il n'y a rien à installer ni à configurer avant.

![La visite de Winlux : lancer un programme Windows](img:lancer-un-exe.jpg "La visite de premier démarrage montre comment lancer un programme Windows.")

## Installer un logiciel

1. Téléchargez l'installateur sur le site de l'éditeur (`setup.exe`, `installer.exe` ou `.msi`).
2. Double-cliquez dessus et suivez les étapes de son installateur.
3. Le logiciel apparaît dans le menu Démarrer. Pour le retirer, ouvrez **Applications installées**.

Les installateurs les plus courants (MSI, NSIS, Inno Setup) sont pris en charge. Le **Store** de Winlux propose aussi des logiciels Windows et Linux à installer en un clic.

## Les autorisations

La première fois qu'un programme veut utiliser la **caméra**, le **micro**, la **capture d'écran** ou vos **dossiers personnels**, Winlux vous demande votre accord. Vous pouvez changer d'avis à tout moment dans Paramètres › Confidentialité et sécurité, où l'accès au réseau peut aussi être bloqué programme par programme.

Quand un installateur demande les droits d'administrateur, Winlux ouvre une fenêtre de confirmation.

## Si un programme ne s'ouvre pas

| Message | Ce que ça veut dire |
|---|---|
| « Programme Windows 32 bits » | Seuls les programmes 64 bits sont pris en charge aujourd'hui. Cherchez la version 64 bits du logiciel. |
| « Fonction manquante » | Le programme utilise une fonction de Windows que Winlux ne fournit pas encore. Signalez-le : c'est ce qui nous dit quoi écrire ensuite. |
| Le programme se ferme sans rien dire | Ouvrez l'Observateur d'événements depuis le menu Démarrer : la cause y est notée. |

Après un plantage, Winlux propose de **signaler le problème** : voir [Obtenir de l'aide](page:guide-help). La liste des logiciels déjà essayés est sur la page [Applications testées](page:apps).

Pour aller plus loin : [Compatibilité des applications](wiki:Compatibilite-des-applications).
